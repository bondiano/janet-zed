//! `janet-zed-server kernel <connection_file> <janet>`: a Jupyter kernel for Zed's REPL.
//! Code runs in a shared netrepl process, so terminal clients (`netrepl/client`) see the same state.

pub mod lookup;
pub mod netrepl;
mod snippet;

use std::fs;
use std::path::{Path, PathBuf};

use jupyter_protocol::{
    ConnectionInfo, ErrorOutput, ExecuteInput, ExecuteReply, ExecuteResult, ExecutionCount,
    JupyterMessage, JupyterMessageContent, KernelInfoReply, LanguageInfo, MediaType, ReplyError,
    ReplyStatus, ShutdownReply, Status, StreamContent,
};
use jupyter_zmq_client::{
    KernelIoPubConnection, KernelShellConnection, create_kernel_control_connection,
    create_kernel_heartbeat_connection, create_kernel_iopub_connection,
    create_kernel_shell_connection, create_kernel_stdin_connection, user_data_dir,
};
use serde_json::json;

use netrepl::{Evaluation, HOST, Netrepl, PORT};

use anyhow::Result;

/// Writes the kernelspec Zed discovers; its `language` matches the Janet language name.
pub fn register(janet: &str) -> Result<PathBuf> {
    let dir = user_data_dir()?.join("kernels/janet-zed");
    fs::create_dir_all(&dir)?;
    let spec = json!({
        "argv": [std::env::current_exe()?, "kernel", "{connection_file}", janet],
        "display_name": "Janet",
        "language": "janet",
    });
    fs::write(
        dir.join("kernel.json"),
        serde_json::to_string_pretty(&spec)?,
    )?;
    Ok(dir)
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
    // ponytail: interrupts are ignored; a runaway eval needs a kernel restart.
    tokio::spawn(async move {
        while let Ok(request) = control.read().await {
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
                let evaluation = evaluate(&mut repl, janet, &execute.code).await;
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

/// Evaluates over the cached connection; a broken connection is dropped so the next request reconnects.
async fn evaluate(repl: &mut Option<Netrepl>, janet: &str, code: &str) -> Evaluation {
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
        None => match Netrepl::connect(janet, PORT).await {
            Ok(connection) => repl.insert(connection).eval(code, position.as_ref()).await,
            Err(err) => Err(err),
        },
    };
    result.unwrap_or_else(|err| {
        *repl = None;
        Evaluation {
            errors: format!("netrepl at {HOST}:{PORT}: {err}"),
            ..Evaluation::default()
        }
    })
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
        banner: format!("Janet via netrepl at {HOST}:{PORT}"),
        help_links: vec![],
        debugger: false,
        error: None,
    }
}
