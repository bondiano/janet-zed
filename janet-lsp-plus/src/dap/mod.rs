//! `janet-lsp-plus dap`: the debug adapter behind Zed's debugger.
//!
//! Zed speaks DAP over stdio. The debuggee is a `janet` running `driver.janet`, which connects
//! back over TCP: the adapter writes it one Janet form per line and reads JSON lines back.
//! Requests about a stop (stack, variables, evaluation, stepping) go to the driver; the session
//! lifecycle, threads and breakpoint bookkeeping stay here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::kernel::netrepl::{self, Netrepl};

const DRIVER: &str = include_str!("driver.janet");
/// The JSON encoder the driver prints its replies with.
const JSON: &str = janet_check::janet::JSON;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Requests the driver answers while the program is stopped.
const STOP_REQUESTS: [&str; 8] = [
    "stackTrace",
    "scopes",
    "variables",
    "evaluate",
    "continue",
    "next",
    "stepIn",
    "stepOut",
];

enum Input {
    /// A request from Zed; `None` when stdin closes.
    Client(Option<Request>),
    /// A line from the driver; `None` when its connection closes.
    Driver(Option<Value>),
    Output(&'static str, String),
    Exited(Option<i32>),
}

#[derive(Deserialize)]
struct Request {
    seq: i64,
    #[serde(rename = "type")]
    kind: String,
    command: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchArguments {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    cwd: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    janet: Option<String>,
    #[serde(default)]
    stop_on_entry: bool,
}

/// The netrepl server of the REPL kernel.
#[derive(Deserialize)]
struct AttachArguments {
    #[serde(default = "default_host")]
    host: String,
    /// The port the REPL kernel recorded for the project of `cwd` when not given.
    port: Option<u16>,
    /// The extension sets it to the worktree root.
    cwd: Option<PathBuf>,
}

fn default_host() -> String {
    netrepl::HOST.to_string()
}

enum Target {
    Launch(LaunchArguments),
    Attach(AttachArguments),
}

#[derive(Deserialize)]
struct SetBreakpointsArguments {
    source: Source,
    #[serde(default)]
    breakpoints: Vec<SourceBreakpoint>,
}

#[derive(Deserialize)]
struct Source {
    path: Option<String>,
}

#[derive(Deserialize)]
struct SourceBreakpoint {
    line: i64,
}

#[derive(Deserialize)]
struct SetExceptionBreakpointsArguments {
    filters: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopesArguments {
    frame_id: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VariablesArguments {
    variables_reference: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluateArguments {
    expression: String,
    frame_id: Option<i64>,
}

pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(serve());
    // The stdin reader blocks a thread that a plain drop would wait for.
    runtime.shutdown_background();
    result
}

async fn serve() -> Result<()> {
    let (inputs, mut received) = mpsc::unbounded_channel();
    tokio::spawn(read_client(inputs.clone()));
    let mut session = Session::new(inputs);
    while let Some(input) = received.recv().await {
        match input {
            Input::Client(Some(request)) => {
                if !session.request(request).await? {
                    break;
                }
            }
            Input::Client(None) => break,
            Input::Driver(Some(message)) => session.driver_message(&message).await?,
            Input::Driver(None) => session.driver_closed().await?,
            Input::Output(category, output) => {
                let body = json!({"category": category, "output": output});
                session.event("output", body).await?;
            }
            Input::Exited(code) => session.exited(code).await?,
        }
    }
    Ok(())
}

struct Session {
    stdout: tokio::io::Stdout,
    seq: i64,
    inputs: mpsc::UnboundedSender<Input>,
    target: Option<Target>,
    /// Source path → (id, line) of its breakpoints, as last set.
    breakpoints: HashMap<String, Vec<(i64, i64)>>,
    last_breakpoint_id: i64,
    uncaught: bool,
    driver: Option<OwnedWriteHalf>,
    /// Requests sent to the driver, by seq (the id the driver answers with).
    pending: HashMap<i64, String>,
    stopped: bool,
    kill: Option<oneshot::Sender<()>>,
    /// The netrepl connection an attach session keeps open.
    repl: Option<Netrepl>,
}

impl Session {
    fn new(inputs: mpsc::UnboundedSender<Input>) -> Self {
        Self {
            stdout: tokio::io::stdout(),
            seq: 0,
            inputs,
            target: None,
            breakpoints: HashMap::new(),
            last_breakpoint_id: 0,
            uncaught: true,
            driver: None,
            pending: HashMap::new(),
            stopped: false,
            kill: None,
            repl: None,
        }
    }

    /// Handles a request from Zed. Returns false once the session is over.
    async fn request(&mut self, request: Request) -> Result<bool> {
        let Request {
            seq,
            kind,
            command,
            arguments,
        } = request;
        if kind != "request" {
            return Ok(true);
        }
        let result = match command.as_str() {
            "initialize" => Ok(capabilities()),
            "launch" => parse(arguments).map(|launch| {
                self.target = Some(Target::Launch(launch));
                json!({})
            }),
            "attach" => parse(arguments).map(|attach| {
                // A REPL error should not stop: `uncaught` stays off unless turned on later.
                self.uncaught = false;
                self.target = Some(Target::Attach(attach));
                json!({})
            }),
            "setBreakpoints" => self.set_breakpoints(arguments).await,
            "setExceptionBreakpoints" => self.set_exceptions(arguments).await,
            "configurationDone" => self.start().await.map(|()| json!({})),
            "threads" => Ok(json!({"threads": [{"id": 1, "name": "main"}]})),
            "disconnect" | "terminate" => {
                if let Some(kill) = self.kill.take() {
                    kill.send(()).ok();
                }
                Ok(json!({}))
            }
            name if STOP_REQUESTS.contains(&name) && self.stopped => {
                match self.forward(seq, name, arguments).await {
                    // The driver answers.
                    Ok(()) => return Ok(true),
                    Err(err) => Err(err),
                }
            }
            name if STOP_REQUESTS.contains(&name) => Err(anyhow!("the program is not stopped")),
            name => Err(anyhow!("unsupported request `{name}`")),
        };
        let failed = result.is_err();
        self.respond(seq, &command, result).await?;
        match command.as_str() {
            // Zed configures breakpoints once the launch or attach request is in.
            "launch" | "attach" if !failed => self.event("initialized", json!({})).await?,
            "configurationDone" if failed => self.event("terminated", json!({})).await?,
            _ => {}
        }
        Ok(command != "disconnect")
    }

    async fn set_breakpoints(&mut self, arguments: Value) -> Result<Value> {
        let arguments: SetBreakpointsArguments = parse(arguments)?;
        let path = arguments
            .source
            .path
            .context("a breakpoint source without a path")?;
        let breakpoints: Vec<(i64, i64)> = arguments
            .breakpoints
            .iter()
            .map(|breakpoint| {
                self.last_breakpoint_id += 1;
                (self.last_breakpoint_id, breakpoint.line)
            })
            .collect();
        // Unverified until the driver finds compiled code on the line.
        let body = json!({
            "breakpoints": breakpoints
                .iter()
                .map(|(id, line)| json!({"id": id, "line": line, "verified": false}))
                .collect::<Vec<_>>(),
        });
        if self.driver.is_some() {
            self.send_driver(&breakpoints_command(&path, &breakpoints))
                .await?;
        }
        self.breakpoints.insert(path, breakpoints);
        Ok(body)
    }

    async fn set_exceptions(&mut self, arguments: Value) -> Result<Value> {
        let arguments: SetExceptionBreakpointsArguments = parse(arguments)?;
        // Zed sends the filter defaults before an attach starts; those keep `uncaught` off.
        if matches!(self.target, Some(Target::Attach(_))) {
            return Ok(json!({}));
        }
        self.uncaught = arguments.filters.iter().any(|filter| filter == "uncaught");
        if self.driver.is_some() {
            self.send_driver(&format!("[0 :exceptions {}]", self.uncaught))
                .await?;
        }
        Ok(json!({}))
    }

    /// Starts the session the launch or attach request configured.
    async fn start(&mut self) -> Result<()> {
        match self.target.take().context("no launch or attach request")? {
            Target::Launch(launch) => self.launch(launch).await,
            Target::Attach(attach) => self.attach(attach).await,
        }
    }

    /// Spawns `janet` with the driver, waits for it to connect and hands it the program.
    async fn launch(&mut self, launch: LaunchArguments) -> Result<()> {
        let janet = launch.janet.as_deref().unwrap_or("janet");
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let script = format!(
            "{JSON}{DRIVER}\n(driver/launch \"{}\")",
            listener.local_addr()?.port()
        );
        let mut command = Command::new(janet);
        command
            .args(["-e", &script])
            .envs(&launch.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &launch.cwd {
            command.current_dir(cwd);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("running `{janet}`"))?;
        let outputs = [
            forward_output(child.stdout.take(), "stdout", self.inputs.clone()),
            forward_output(child.stderr.take(), "stderr", self.inputs.clone()),
        ];
        let stream = tokio::select! {
            accepted = listener.accept() => accepted?.0,
            status = child.wait() => bail!("`{janet}` exited before the debugger connected ({})", status?),
            () = tokio::time::sleep(CONNECT_TIMEOUT) => bail!("`{janet}` did not connect in {CONNECT_TIMEOUT:?}"),
        };

        let (kill, killed) = oneshot::channel();
        tokio::spawn(watch(child, killed, outputs, self.inputs.clone()));
        self.kill = Some(kill);

        let args: Vec<String> = launch.args.iter().map(|arg| janet_string(arg)).collect();
        let program = format!(
            "[0 :launch {} [{}] {}]",
            janet_string(&launch.program),
            args.join(" "),
            launch.stop_on_entry
        );
        self.connect_driver(stream, Some(program)).await
    }

    /// Runs the driver inside the REPL's netrepl process, next to the kernel's evaluations.
    async fn attach(&mut self, attach: AttachArguments) -> Result<()> {
        let mut repl = if let Some(port) = attach.port {
            let address = format!("{}:{port}", attach.host);
            Netrepl::attach(&address, "zed-dap")
                .await
                .with_context(|| format!("no netrepl at {address}"))?
        } else {
            let cwd = match attach.cwd {
                Some(cwd) => cwd,
                None => std::env::current_dir()?,
            };
            Netrepl::attach_recorded(&netrepl::project_of(&cwd))
                .await
                .context("no REPL for this project: start the Janet REPL kernel first")?
        };
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        // ponytail: the driver connects back to 127.0.0.1, so only a REPL on this machine attaches.
        let script = format!(
            "{JSON}{DRIVER}\n(driver/attach \"{}\")",
            listener.local_addr()?.port()
        );
        let reply = repl
            .call(&format!("(eval-string {})", janet_string(&script)))
            .await?;
        if !reply.starts_with("(true") {
            bail!("the REPL could not start the debugger: {reply}");
        }
        let (stream, _) = tokio::time::timeout(CONNECT_TIMEOUT, listener.accept())
            .await
            .context("the REPL did not connect to the debugger")??;
        self.repl = Some(repl);
        self.connect_driver(stream, None).await
    }

    /// Takes the driver's connection and sends it the breakpoints so far, then `last`.
    async fn connect_driver(&mut self, stream: TcpStream, last: Option<String>) -> Result<()> {
        let mut commands: Vec<String> = self
            .breakpoints
            .iter()
            .map(|(path, breakpoints)| breakpoints_command(path, breakpoints))
            .collect();
        commands.push(format!("[0 :exceptions {}]", self.uncaught));
        commands.extend(last);
        let (read, write) = stream.into_split();
        tokio::spawn(read_driver(read, self.inputs.clone()));
        self.driver = Some(write);
        for command in commands {
            self.send_driver(&command).await?;
        }
        Ok(())
    }

    /// Sends a request about the stop to the driver as `[seq :command args…]`.
    async fn forward(&mut self, seq: i64, command: &str, arguments: Value) -> Result<()> {
        let args = match command {
            "scopes" => parse::<ScopesArguments>(arguments)?.frame_id.to_string(),
            "variables" => parse::<VariablesArguments>(arguments)?
                .variables_reference
                .to_string(),
            "evaluate" => {
                let arguments: EvaluateArguments = parse(arguments)?;
                let frame = arguments
                    .frame_id
                    .map_or_else(|| "nil".to_string(), |id| id.to_string());
                format!("{frame} {}", janet_string(&arguments.expression))
            }
            _ => String::new(),
        };
        self.send_driver(&format!("[{seq} :{command} {args}]"))
            .await?;
        self.pending.insert(seq, command.to_string());
        if matches!(command, "continue" | "next" | "stepIn" | "stepOut") {
            self.stopped = false;
        }
        Ok(())
    }

    async fn driver_message(&mut self, message: &Value) -> Result<()> {
        // Events carry ids too (a breakpoint's), so they are told apart first.
        if let (None, Some(id)) = (message.get("event"), message["id"].as_i64()) {
            let Some(command) = self.pending.remove(&id) else {
                return Ok(());
            };
            let result = match message.get("error") {
                Some(error) => Err(anyhow!("{}", error.as_str().unwrap_or("driver error"))),
                None => Ok(message["body"].clone()),
            };
            return self.respond(id, &command, result).await;
        }
        match message["event"].as_str() {
            Some("stopped") => {
                self.stopped = true;
                let mut body = json!({
                    "reason": message["reason"],
                    "threadId": 1,
                    "allThreadsStopped": true,
                });
                if let Some(text) = message["text"].as_str() {
                    body["text"] = text.into();
                    body["description"] = text.lines().next().unwrap_or_default().into();
                }
                self.event("stopped", body).await
            }
            Some("breakpoint") => {
                let breakpoint = json!({
                    "id": message["id"],
                    "line": message["line"],
                    "verified": message["verified"],
                });
                self.event(
                    "breakpoint",
                    json!({"reason": "changed", "breakpoint": breakpoint}),
                )
                .await
            }
            _ => {
                tracing::warn!("unexpected driver message: {message}");
                Ok(())
            }
        }
    }

    async fn exited(&mut self, code: Option<i32>) -> Result<()> {
        self.stopped = false;
        self.driver = None;
        self.kill = None;
        self.event("exited", json!({"exitCode": code.unwrap_or(-1)}))
            .await?;
        self.event("terminated", json!({})).await
    }

    /// A launched program reports its exit on its own; a REPL that goes away ends the session.
    async fn driver_closed(&mut self) -> Result<()> {
        self.driver = None;
        if self.repl.take().is_some() {
            self.stopped = false;
            self.event("terminated", json!({})).await?;
        }
        Ok(())
    }

    async fn send_driver(&mut self, line: &str) -> Result<()> {
        let driver = self.driver.as_mut().context("the program is not running")?;
        driver.write_all(format!("{line}\n").as_bytes()).await?;
        Ok(())
    }

    async fn respond(
        &mut self,
        request_seq: i64,
        command: &str,
        result: Result<Value>,
    ) -> Result<()> {
        let mut message = json!({
            "type": "response",
            "request_seq": request_seq,
            "success": result.is_ok(),
            "command": command,
        });
        match result {
            Ok(body) => message["body"] = body,
            Err(err) => message["message"] = format!("{err:#}").into(),
        }
        self.send(message).await
    }

    async fn event(&mut self, event: &str, body: Value) -> Result<()> {
        self.send(json!({"type": "event", "event": event, "body": body}))
            .await
    }

    async fn send(&mut self, mut message: Value) -> Result<()> {
        self.seq += 1;
        message["seq"] = self.seq.into();
        let body = serde_json::to_vec(&message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdout.write_all(header.as_bytes()).await?;
        self.stdout.write_all(&body).await?;
        self.stdout.flush().await?;
        Ok(())
    }
}

fn capabilities() -> Value {
    json!({
        "supportsConfigurationDoneRequest": true,
        "supportsEvaluateForHovers": true,
        "exceptionBreakpointFilters": [
            {"filter": "uncaught", "label": "Uncaught errors", "default": true},
        ],
    })
}

fn parse<T: DeserializeOwned>(arguments: Value) -> Result<T> {
    serde_json::from_value(arguments).context("invalid request arguments")
}

/// A JSON string is a valid Janet string literal.
fn janet_string(text: &str) -> String {
    Value::from(text).to_string()
}

fn breakpoints_command(path: &str, breakpoints: &[(i64, i64)]) -> String {
    let pairs: Vec<String> = breakpoints
        .iter()
        .map(|(id, line)| format!("[{id} {line}]"))
        .collect();
    format!(
        "[0 :breakpoints {} [{}]]",
        janet_string(path),
        pairs.join(" ")
    )
}

async fn read_client(inputs: mpsc::UnboundedSender<Input>) {
    let mut stdin = BufReader::new(tokio::io::stdin());
    loop {
        match read_message(&mut stdin).await {
            Ok(Some(request)) => {
                inputs.send(Input::Client(Some(request))).ok();
            }
            Ok(None) => break,
            Err(err) => {
                tracing::error!("reading a DAP message: {err:#}");
                break;
            }
        }
    }
    inputs.send(Input::Client(None)).ok();
}

/// One `Content-Length`-framed message, or `None` at the end of the stream.
async fn read_message(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Option<Request>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = Some(value.trim().parse()?);
        }
    }
    let mut body = vec![0; length.context("a DAP message without Content-Length")?];
    reader.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

async fn read_driver(read: OwnedReadHalf, inputs: mpsc::UnboundedSender<Input>) {
    let mut reader = BufReader::new(read);
    let mut line = Vec::new();
    while reader
        .read_until(b'\n', &mut line)
        .await
        .is_ok_and(|read| read > 0)
    {
        // Values may hold bytes that are not UTF-8.
        match serde_json::from_str(&String::from_utf8_lossy(&line)) {
            Ok(message) => {
                inputs.send(Input::Driver(Some(message))).ok();
            }
            Err(err) => tracing::warn!("unexpected driver line: {err}"),
        }
        line.clear();
    }
    inputs.send(Input::Driver(None)).ok();
}

fn forward_output(
    pipe: Option<impl AsyncRead + Unpin + Send + 'static>,
    category: &'static str,
    inputs: mpsc::UnboundedSender<Input>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(mut pipe) = pipe else { return };
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        while let Ok(read @ 1..) = pipe.read(&mut chunk).await {
            bytes.extend_from_slice(&chunk[..read]);
            // A character split between reads waits for its other bytes.
            let complete = match std::str::from_utf8(&bytes) {
                Err(err) if err.error_len().is_none() => err.valid_up_to(),
                _ => bytes.len(),
            };
            let output = String::from_utf8_lossy(&bytes[..complete]).into_owned();
            bytes.drain(..complete);
            if !output.is_empty() {
                inputs.send(Input::Output(category, output)).ok();
            }
        }
    })
}

/// Waits for `janet` to exit (or kills it), then for its output, and reports the exit.
async fn watch(
    mut child: Child,
    killed: oneshot::Receiver<()>,
    outputs: [JoinHandle<()>; 2],
    inputs: mpsc::UnboundedSender<Input>,
) {
    let status = tokio::select! {
        status = child.wait() => status.ok(),
        _ = killed => {
            child.kill().await.ok();
            child.wait().await.ok()
        }
    };
    for output in outputs {
        output.await.ok();
    }
    inputs
        .send(Input::Exited(status.and_then(|status| status.code())))
        .ok();
}
