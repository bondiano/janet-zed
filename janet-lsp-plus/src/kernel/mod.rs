//! `janet-lsp-plus kernel <connection_file> <janet>`: a Jupyter kernel for Zed's REPL.
//! Code runs in a netrepl process of the kernel's own, so terminal clients (`netrepl/client`) see
//! the same state. Its port is recorded per worktree, where the language server, the debugger and
//! the terminal task look it up.

pub mod lookup;
pub mod netrepl;
mod snippet;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use jupyter_protocol::{
    CompleteReply, ConnectionInfo, ErrorOutput, ExecuteInput, ExecuteReply, ExecuteResult,
    ExecutionCount, InputRequest, InspectReply, InterruptReply, IsCompleteReply,
    IsCompleteReplyStatus, JupyterMessage, JupyterMessageContent, KernelInfoReply, LanguageInfo,
    MediaType, ReplyError, ReplyStatus, ShutdownReply, Status, StreamContent,
};
use jupyter_zmq_client::{
    KernelIoPubConnection, KernelShellConnection, KernelStdinConnection,
    create_kernel_control_connection, create_kernel_heartbeat_connection,
    create_kernel_iopub_connection, create_kernel_shell_connection, create_kernel_stdin_connection,
    user_data_dir,
};
use serde_json::json;

use netrepl::{Evaluation, Message, Netrepl, jdn_strings};

use anyhow::Result;

/// Where the kernelspec Zed discovers lives.
fn kernel_dir() -> Result<PathBuf> {
    Ok(user_data_dir()?.join("kernels/janet-zed"))
}

/// Writes the kernelspec Zed discovers; its `language` matches the Janet language name.
pub fn register(janet: &str) -> Result<PathBuf> {
    let dir = kernel_dir()?;
    install(&dir, &std::env::current_exe()?, janet)?;
    Ok(dir)
}

/// Removes the kernelspec, where there is one.
pub fn unregister() -> Result<()> {
    match fs::remove_dir_all(kernel_dir()?) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.into()),
        _ => Ok(()),
    }
}

/// The kernelspec in `dir`, starting a copy of `exe` kept beside it. The binary the server runs
/// from lives in a directory named after its version, which the extension deletes once it
/// downloads the next one: a spec pointing there would outlive its kernel.
fn install(dir: &Path, exe: &Path, janet: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    let kernel = dir.join(format!("janet-lsp-plus{}", std::env::consts::EXE_SUFFIX));
    if !is_copy_of(&kernel, exe)? {
        // Renamed into place: a kernel running from the old copy keeps its file. The name is the
        // process's own, so two servers starting at once do not write into one file.
        let fresh = kernel.with_extension(format!("new-{}", std::process::id()));
        fs::copy(exe, &fresh)?;
        if let Err(err) = fs::rename(&fresh, &kernel) {
            fs::remove_file(&fresh).ok();
            return Err(err.into());
        }
    }
    let spec = json!({
        "argv": [kernel, "kernel", "{connection_file}", janet],
        "display_name": "Janet",
        "language": "janet",
        // Interrupts come as control messages: a SIGINT would end the kernel.
        "interrupt_mode": "message",
    });
    fs::write(
        dir.join("kernel.json"),
        serde_json::to_string_pretty(&spec)?,
    )?;
    Ok(())
}

/// Whether `kernel` is a copy of `exe` already: of its size, and no older. A binary replaced
/// later is newer than the copy made of the one before.
fn is_copy_of(kernel: &Path, exe: &Path) -> Result<bool> {
    let exe = fs::metadata(exe)?;
    Ok(fs::metadata(kernel).is_ok_and(|kernel| {
        kernel.len() == exe.len()
            && matches!((kernel.modified(), exe.modified()), (Ok(copied), Ok(built)) if copied >= built)
    }))
}

pub fn run(connection_file: &Path, janet: &str) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(serve(connection_file, janet))
}

async fn serve(connection_file: &Path, janet: &str) -> Result<()> {
    let info: ConnectionInfo = serde_json::from_str(&fs::read_to_string(connection_file)?)?;
    let session = format!("janet-zed-{}", std::process::id());
    let mut heartbeat = create_kernel_heartbeat_connection(&info).await?;
    let mut control = create_kernel_control_connection(&info, &session).await?;
    let mut shell = create_kernel_shell_connection(&info, &session).await?;
    let mut iopub = create_kernel_iopub_connection(&info, &session).await?;
    let mut stdin = create_kernel_stdin_connection(&info, &session).await?;

    tokio::spawn(async move { while heartbeat.single_heartbeat().await.is_ok() {} });
    let mut repl = Repl::new(janet);
    let interrupted = Arc::clone(&repl.server);
    tokio::spawn(async move {
        while let Ok(request) = control.read().await {
            if let JupyterMessageContent::InterruptRequest(_) = &request.content {
                let error = interrupt(interrupted.load(Ordering::Relaxed)).err();
                let reply = InterruptReply {
                    status: if error.is_some() {
                        ReplyStatus::Error
                    } else {
                        ReplyStatus::Ok
                    },
                    error: error.map(|err| {
                        Box::new(ReplyError {
                            ename: "interrupt".to_string(),
                            evalue: err.to_string(),
                            traceback: vec![],
                        })
                    }),
                };
                control.send(reply.as_child_of(&request)).await.ok();
            }
            if let JupyterMessageContent::ShutdownRequest(shutdown) = &request.content {
                let reply = ShutdownReply {
                    restart: shutdown.restart,
                    status: ReplyStatus::Ok,
                    error: None,
                };
                control.send(reply.as_child_of(&request)).await.ok();
                // Exiting closes the netrepl server's stdin, which stops it.
                std::process::exit(0);
            }
        }
    });

    let mut count = ExecutionCount::new(0);
    loop {
        let request = shell.read().await?;
        iopub.send(Status::busy().as_child_of(&request)).await?;
        match &request.content {
            JupyterMessageContent::KernelInfoRequest(_) => {
                shell.send(kernel_info().as_child_of(&request)).await?;
            }
            JupyterMessageContent::ExecuteRequest(execute) => {
                count.increment();
                let input = ExecuteInput {
                    code: execute.code.clone(),
                    execution_count: count,
                };
                iopub.send(input.as_child_of(&request)).await?;
                let mut io = Io {
                    iopub: &mut iopub,
                    stdin: execute.allow_stdin.then_some(&mut stdin),
                    request: &request,
                };
                let evaluation = repl.execute(&execute.code, &mut io).await;
                publish(&mut iopub, &mut shell, &request, count, evaluation).await?;
            }
            JupyterMessageContent::CompleteRequest(complete) => {
                let reply = repl.complete(&complete.code, complete.cursor_pos).await;
                shell.send(reply.as_child_of(&request)).await?;
            }
            JupyterMessageContent::InspectRequest(inspect) => {
                let reply = repl.inspect(&inspect.code, inspect.cursor_pos).await;
                shell.send(reply.as_child_of(&request)).await?;
            }
            JupyterMessageContent::IsCompleteRequest(is_complete) => {
                let reply = repl.is_complete(&is_complete.code).await;
                shell.send(reply.as_child_of(&request)).await?;
            }
            _ => {}
        }
        iopub.send(Status::idle().as_child_of(&request)).await?;
    }
}

/// Cancels the evaluation in progress in the server with process id `pid`, if any.
fn interrupt(pid: u32) -> std::io::Result<()> {
    if pid == 0 {
        return Ok(());
    }
    netrepl::signal(pid, "INT")
}

/// Where an evaluation's output and prompts go: the frontend of `request`, whose stdin is there
/// when it allows input.
struct Io<'a> {
    iopub: &'a mut KernelIoPubConnection,
    stdin: Option<&'a mut KernelStdinConnection>,
    request: &'a JupyterMessage,
}

impl Io<'_> {
    async fn output(&mut self, text: &str) -> Result<()> {
        let stream = StreamContent::stdout(text).as_child_of(self.request);
        Ok(self.iopub.send(stream).await?)
    }

    /// A line from the frontend, its newline included; empty, the end of input, when it allows none.
    async fn input(&mut self, prompt: String) -> Result<String> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Ok(String::new());
        };
        let request = InputRequest {
            prompt,
            password: false,
        };
        stdin.send(request.as_child_of(self.request)).await?;
        loop {
            if let JupyterMessageContent::InputReply(reply) = stdin.read().await?.content {
                return Ok(format!("{}\n", reply.value));
            }
        }
    }
}

/// The project's REPL, connected on first use; a broken connection is dropped so the next
/// request reconnects.
struct Repl {
    connection: Option<Netrepl>,
    janet: String,
    /// The process of the server evaluations go to, once connected; 0 before.
    server: Arc<AtomicU32>,
}

impl Repl {
    fn new(janet: &str) -> Self {
        Self {
            connection: None,
            janet: janet.to_string(),
            server: Arc::new(AtomicU32::new(0)),
        }
    }

    async fn connection(&mut self) -> std::io::Result<&mut Netrepl> {
        if let Some(connection) = self.connection.take() {
            return Ok(self.connection.insert(connection));
        }
        let mut connection = start(&self.janet).await?;
        self.server
            .store(connection.pid().await.unwrap_or(0), Ordering::Relaxed);
        Ok(self.connection.insert(connection))
    }

    /// Evaluates `code`, its output and prompts going to `io` as they come.
    async fn execute(&mut self, code: &str, io: &mut Io<'_>) -> Evaluation {
        // Zed starts kernels in the worktree root.
        let position = std::env::current_dir()
            .ok()
            .and_then(|root| snippet::locate(&root, code));
        let code = if position.is_some() {
            code.trim()
        } else {
            code
        };
        let evaluation = async {
            let repl = self.connection().await?;
            repl.begin_eval(code, position.as_ref()).await?;
            let mut message = repl.next().await?;
            loop {
                message = match message {
                    Message::Output(text) => {
                        io.output(&text).await?;
                        repl.next().await?
                    }
                    Message::Input(prompt) => tokio::select! {
                        line = io.input(prompt) => {
                            repl.answer(&line?).await?;
                            repl.next().await?
                        }
                        // An interrupt ends the evaluation while it waits for the line.
                        message = repl.next() => message?,
                    },
                    Message::Done(evaluation) => return anyhow::Ok(evaluation),
                }
            }
        };
        evaluation.await.unwrap_or_else(|err| {
            self.connection = None;
            Evaluation {
                errors: format!("netrepl: {err}"),
                ..Evaluation::default()
            }
        })
    }

    /// The reply of the REPL to `form`, as JDN.
    async fn call(&mut self, form: &str) -> std::io::Result<String> {
        let reply = self.connection().await?.call(form).await;
        if reply.is_err() {
            self.connection = None;
        }
        reply
    }

    /// The names the REPL has for the symbol before `cursor`.
    async fn complete(&mut self, code: &str, cursor: usize) -> CompleteReply {
        let (start, end) = symbol_at(code, cursor, false);
        let prefix: String = code.chars().skip(start).take(end - start).collect();
        let form = format!(
            "(do (def names @{{}}) (var env (curenv))
               (while env
                 (eachk name env
                   (when (and (symbol? name) (string/has-prefix? {} name)) (put names name true)))
                 (set env (table/getproto env)))
               (sort (map string (keys names))))",
            serde_json::Value::from(prefix)
        );
        let reply = self.call(&form).await;
        CompleteReply {
            matches: reply.as_deref().map(jdn_strings).unwrap_or_default(),
            cursor_start: start,
            cursor_end: end,
            metadata: serde_json::Map::default(),
            status: reply_status(reply.as_ref()),
            error: reply_error(reply.err()),
        }
    }

    /// The docs of the symbol around `cursor`, as `doc` prints them.
    async fn inspect(&mut self, code: &str, cursor: usize) -> InspectReply {
        let (start, end) = symbol_at(code, cursor, true);
        let name: String = code.chars().skip(start).take(end - start).collect();
        let name = serde_json::Value::from(name);
        let form = format!(
            "(when (and (not (empty? {name})) (dyn (symbol {name})))
               (def buf @\"\") (with-dyns [:out buf] (doc* (symbol {name}))) (string buf))"
        );
        let reply = self.call(&form).await;
        let doc = reply.as_deref().ok().and_then(|reply| {
            reply
                .starts_with("(true")
                .then(|| jdn_strings(reply).pop())
                .flatten()
        });
        InspectReply {
            found: doc.is_some(),
            data: doc
                .map(|doc| MediaType::Plain(doc.trim_matches('\n').to_string()).into())
                .unwrap_or_default(),
            metadata: serde_json::Map::default(),
            status: reply_status(reply.as_ref()),
            error: reply_error(reply.err()),
        }
    }

    /// Whether `code` is whole forms, as Janet's parser takes it.
    async fn is_complete(&mut self, code: &str) -> IsCompleteReply {
        let form = format!(
            "(let [p (parser/new)] (parser/consume p {}) (parser/status p))",
            serde_json::Value::from(code)
        );
        let (status, indent) = match self.call(&form).await.as_deref() {
            Ok("(true :root)") => (IsCompleteReplyStatus::Complete, ""),
            Ok("(true :pending)") => (IsCompleteReplyStatus::Incomplete, "  "),
            Ok("(true :error)") => (IsCompleteReplyStatus::Invalid, ""),
            _ => (IsCompleteReplyStatus::Unknown, ""),
        };
        IsCompleteReply {
            status,
            indent: indent.to_string(),
        }
    }
}

fn reply_status<T>(reply: Result<&T, &std::io::Error>) -> ReplyStatus {
    if reply.is_ok() {
        ReplyStatus::Ok
    } else {
        ReplyStatus::Error
    }
}

fn reply_error(error: Option<std::io::Error>) -> Option<Box<ReplyError>> {
    error.map(|err| {
        Box::new(ReplyError {
            ename: "netrepl".to_string(),
            evalue: err.to_string(),
            traceback: vec![],
        })
    })
}

/// The span, in characters as Jupyter counts the cursor, of the symbol that ends at `cursor`, or
/// that holds it when `around`.
fn symbol_at(code: &str, cursor: usize, around: bool) -> (usize, usize) {
    let chars: Vec<char> = code.chars().collect();
    let cursor = cursor.min(chars.len());
    let is_symbol = |c: &char| !c.is_whitespace() && !"()[]{}\"'`~,;|@".contains(*c);
    let start = cursor
        - chars[..cursor]
            .iter()
            .rev()
            .take_while(|c| is_symbol(c))
            .count();
    let end = if around {
        cursor + chars[cursor..].iter().take_while(|c| is_symbol(c)).count()
    } else {
        cursor
    };
    (start, end)
}

/// The REPL of the project the kernel runs in: the one another kernel started for it, while that
/// one runs, else a netrepl server of its own on a free port, recorded for the project.
async fn start(janet: &str) -> std::io::Result<Netrepl> {
    let project = netrepl::project_of(&std::env::current_dir()?);
    if let Ok(connection) = Netrepl::attach_recorded(&project).await {
        return Ok(connection);
    }
    let port = netrepl::free_port()?;
    let token = netrepl::new_token()?;
    let connection = Netrepl::start(janet, port, &project, &token).await?;
    netrepl::record(&project, port, &token)?;
    Ok(connection)
}

/// Publishes the end of an evaluation whose input and output went out already.
async fn publish(
    iopub: &mut KernelIoPubConnection,
    shell: &mut KernelShellConnection,
    request: &JupyterMessage,
    execution_count: ExecutionCount,
    Evaluation {
        value,
        output,
        errors,
    }: Evaluation,
) -> Result<()> {
    if !output.is_empty() {
        iopub
            .send(StreamContent::stdout(&output).as_child_of(request))
            .await?;
    }

    let error = (!errors.is_empty()).then(|| ReplyError {
        ename: "error".to_string(),
        evalue: errors.lines().next().unwrap_or_default().to_string(),
        traceback: errors.lines().map(str::to_string).collect(),
    });
    match &error {
        Some(ReplyError {
            ename,
            evalue,
            traceback,
        }) => {
            let content = ErrorOutput {
                ename: ename.clone(),
                evalue: evalue.clone(),
                traceback: traceback.clone(),
            };
            iopub.send(content.as_child_of(request)).await?;
        }
        None if !value.is_empty() => {
            let result = ExecuteResult {
                execution_count,
                data: MediaType::Plain(value).into(),
                metadata: serde_json::Map::default(),
                transient: None,
            };
            iopub.send(result.as_child_of(request)).await?;
        }
        None => {}
    }

    let reply = ExecuteReply {
        status: if error.is_some() {
            ReplyStatus::Error
        } else {
            ReplyStatus::Ok
        },
        execution_count,
        payload: vec![],
        user_expressions: None,
        error: error.map(Box::new),
    };
    shell.send(reply.as_child_of(request)).await?;
    Ok(())
}

fn kernel_info() -> KernelInfoReply {
    KernelInfoReply {
        status: ReplyStatus::Ok,
        protocol_version: "5.3".to_string(),
        implementation: "janet-zed".to_string(),
        implementation_version: env!("CARGO_PKG_VERSION").to_string(),
        language_info: LanguageInfo {
            name: "janet".to_string(),
            version: String::new(),
            mimetype: Some("text/x-janet".to_string()),
            file_extension: Some(".janet".to_string()),
            pygments_lexer: None,
            codemirror_mode: None,
            nbconvert_exporter: None,
        },
        banner: "Janet via netrepl".to_string(),
        help_links: vec![],
        debugger: false,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn the_symbol_at_the_cursor_counts_characters() {
        let code = "(print (маp inc";
        let cursor = 10;
        assert_eq!(symbol_at(code, cursor, false), (8, 10));
        assert_eq!(symbol_at(code, cursor, true), (8, 11));
        assert_eq!(symbol_at("(map inc", 4, true), (1, 4));
        assert_eq!(symbol_at("", 3, true), (0, 0));
    }

    /// Completion, docs and the parser's verdict come from the REPL.
    #[test]
    fn completes_inspects_and_tells_whole_code() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut repl = Repl::new("janet");
        let port = netrepl::free_port().unwrap();
        repl.connection = Some(
            runtime
                .block_on(Netrepl::start("janet", port, Path::new("/work/app"), "t"))
                .unwrap(),
        );
        runtime.block_on(async {
            let complete = repl.complete("(string/tri", 11).await;
            assert!(
                complete.matches.contains(&"string/trim".to_string()),
                "{complete:?}"
            );
            assert_eq!((complete.cursor_start, complete.cursor_end), (1, 11));

            let inspect = repl.inspect("(map inc [1])", 2).await;
            assert!(inspect.found);
            let data = serde_json::to_string(&inspect.data).unwrap();
            assert!(data.contains("(map f x"), "{data}");
            assert!(!repl.inspect("(nope-nope 1)", 3).await.found);

            let status = |reply: IsCompleteReply| reply.status;
            assert!(matches!(
                status(repl.is_complete("(+ 1 2)").await),
                IsCompleteReplyStatus::Complete
            ));
            assert!(matches!(
                status(repl.is_complete("(+ 1\n").await),
                IsCompleteReplyStatus::Incomplete
            ));
            assert!(matches!(
                status(repl.is_complete(")").await),
                IsCompleteReplyStatus::Invalid
            ));
        });
    }

    #[test]
    fn the_kernelspec_starts_a_copy_that_outlives_the_server_binary() {
        let root =
            std::env::temp_dir().join(format!("janet-zed-kernelspec-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        let versioned = root.join("janet-lsp-plus-v0.3.0");
        fs::create_dir_all(&versioned).unwrap();
        let exe = versioned.join("janet-lsp-plus");
        fs::write(&exe, "v0.3.0").unwrap();
        let dir = root.join("kernels/janet-zed");

        install(&dir, &exe, "janet").unwrap();
        fs::remove_dir_all(&versioned).unwrap();

        let spec: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("kernel.json")).unwrap()).unwrap();
        let kernel = PathBuf::from(spec["argv"][0].as_str().unwrap());
        assert!(kernel.starts_with(&dir));
        assert_eq!(fs::read_to_string(&kernel).unwrap(), "v0.3.0");

        // The next version replaces the copy.
        fs::create_dir_all(&versioned).unwrap();
        fs::write(&exe, "v0.3.10").unwrap();
        install(&dir, &exe, "janet").unwrap();
        assert_eq!(fs::read_to_string(&kernel).unwrap(), "v0.3.10");
        // Nothing is left behind by the copy.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(&root).ok();
    }
}
