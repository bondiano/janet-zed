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
    ConnectionInfo, ErrorOutput, ExecuteInput, ExecuteReply, ExecuteResult, ExecutionCount,
    InterruptReply, JupyterMessage, JupyterMessageContent, KernelInfoReply, LanguageInfo,
    MediaType, ReplyError, ReplyStatus, ShutdownReply, Status, StreamContent,
};
use jupyter_zmq_client::{
    KernelIoPubConnection, KernelShellConnection, create_kernel_control_connection,
    create_kernel_heartbeat_connection, create_kernel_iopub_connection,
    create_kernel_shell_connection, create_kernel_stdin_connection, user_data_dir,
};
use serde_json::json;

use netrepl::{Evaluation, Netrepl};

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
    let _stdin = create_kernel_stdin_connection(&info, &session).await?;

    tokio::spawn(async move { while heartbeat.single_heartbeat().await.is_ok() {} });
    // The process of the netrepl server evaluations go to, once connected; 0 before.
    let server = Arc::new(AtomicU32::new(0));
    let interrupted = Arc::clone(&server);
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

    let mut repl = None;
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
                let evaluation = evaluate(&mut repl, &server, janet, &execute.code).await;
                publish(
                    &mut iopub,
                    &mut shell,
                    &request,
                    &execute.code,
                    count,
                    evaluation,
                )
                .await?;
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

/// Evaluates over the cached connection; a broken connection is dropped so the next request
/// reconnects. The process of a new connection's server goes to `server`.
async fn evaluate(
    repl: &mut Option<Netrepl>,
    server: &AtomicU32,
    janet: &str,
    code: &str,
) -> Evaluation {
    // Zed starts kernels in the worktree root.
    let position = std::env::current_dir()
        .ok()
        .and_then(|root| snippet::locate(&root, code));
    let code = if position.is_some() {
        code.trim()
    } else {
        code
    };
    let result = match repl {
        Some(connection) => connection.eval(code, position.as_ref()).await,
        None => match start(janet).await {
            Ok(mut connection) => {
                server.store(connection.pid().await.unwrap_or(0), Ordering::Relaxed);
                repl.insert(connection).eval(code, position.as_ref()).await
            }
            Err(err) => Err(err),
        },
    };
    result.unwrap_or_else(|err| {
        *repl = None;
        Evaluation {
            errors: format!("netrepl: {err}"),
            ..Evaluation::default()
        }
    })
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

async fn publish(
    iopub: &mut KernelIoPubConnection,
    shell: &mut KernelShellConnection,
    request: &JupyterMessage,
    code: &str,
    execution_count: ExecutionCount,
    Evaluation {
        value,
        output,
        errors,
    }: Evaluation,
) -> Result<()> {
    let input = ExecuteInput {
        code: code.to_string(),
        execution_count,
    };
    iopub.send(input.as_child_of(request)).await?;
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
