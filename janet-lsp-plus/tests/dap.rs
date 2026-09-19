//! The debug adapter end to end: `janet-lsp-plus dap` over stdio, debugging
//! the programs in `fixtures/debug` with the real `janet`.

#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use janet_lsp_plus::kernel::netrepl::{Netrepl, Position};
use serde_json::{Value, json};

struct Adapter {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    /// Messages read while waiting for another one.
    backlog: Vec<Value>,
    seq: i64,
    program: String,
}

impl Adapter {
    fn start() -> Self {
        Self::debugging("program.janet")
    }

    /// An adapter for `fixtures/debug/{name}`.
    fn debugging(name: &str) -> Self {
        let program = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/debug")
            .join(name)
            .canonicalize()
            .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_janet-lsp-plus"))
            .arg("dap")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (sender, messages) = channel();
        thread::spawn(move || {
            while let Some(message) = read_message(&mut stdout) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            messages,
            backlog: Vec::new(),
            seq: 0,
            program: program.to_string_lossy().into_owned(),
        }
    }

    /// Runs the fixture with breakpoints on `lines` and the exception filters `filters`.
    fn launch(&mut self, lines: &[u32], filters: &[&str], stop_on_entry: bool) -> Value {
        self.request(
            "initialize",
            json!({"adapterID": "Janet", "linesStartAt1": true}),
        );
        let program = self.program.clone();
        self.request(
            "launch",
            json!({"program": program, "stopOnEntry": stop_on_entry}),
        );
        self.event("initialized");
        let breakpoints = self.request(
            "setBreakpoints",
            json!({
                "source": {"path": program},
                "breakpoints": lines.iter().map(|line| json!({"line": line})).collect::<Vec<_>>(),
            }),
        );
        self.request("setExceptionBreakpoints", json!({"filters": filters}));
        self.request("configurationDone", json!({}));
        breakpoints
    }

    fn send(&mut self, command: &str, arguments: Value) -> i64 {
        self.seq += 1;
        let mut message = json!({"seq": self.seq, "type": "request", "command": command});
        message["arguments"] = arguments;
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
        self.seq
    }

    /// The body of a successful response to `command`.
    fn request(&mut self, command: &str, arguments: Value) -> Value {
        let seq = self.send(command, arguments);
        let response = self.wait(|message| message["request_seq"] == seq);
        assert_eq!(response["success"], true, "{command} failed: {response}");
        response["body"].clone()
    }

    fn event(&mut self, event: &str) -> Value {
        self.wait(|message| message["type"] == "event" && message["event"] == event)["body"].clone()
    }

    fn wait(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.backlog.iter().position(&wanted) {
            return self.backlog.remove(index);
        }
        loop {
            let message = self
                .messages
                .recv_timeout(Duration::from_secs(10))
                .unwrap_or_else(|_| panic!("timed out; received {:#?}", self.backlog));
            if wanted(&message) {
                return message;
            }
            self.backlog.push(message);
        }
    }

    /// Waits for a stop with `reason` and returns the top frames as `name:line`.
    fn stopped(&mut self, reason: &str) -> Vec<String> {
        let stopped = self.event("stopped");
        assert_eq!(stopped["reason"], reason, "{stopped}");
        let trace = self.request("stackTrace", json!({"threadId": 1}));
        trace["stackFrames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| {
                if let Some(path) = frame["source"]["path"].as_str() {
                    assert_eq!(path, self.program);
                }
                format!("{}:{}", frame["name"].as_str().unwrap(), frame["line"])
            })
            .collect()
    }

    /// The locals of the top frame as `name=value`.
    fn locals(&mut self) -> Vec<String> {
        let scopes = self.request("scopes", json!({"frameId": 0}));
        let reference = scopes["scopes"][0]["variablesReference"].clone();
        let variables = self.request("variables", json!({"variablesReference": reference}));
        variables["variables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|variable| {
                format!(
                    "{}={}",
                    variable["name"].as_str().unwrap(),
                    variable["value"].as_str().unwrap()
                )
            })
            .collect()
    }

    fn step(&mut self, command: &str) {
        self.request(command, json!({"threadId": 1}));
    }

    /// Output of the program so far, per category, with the `\r\n` Janet prints on Windows as
    /// `\n`.
    fn output(&self, category: &str) -> String {
        self.backlog
            .iter()
            .filter(|message| {
                message["event"] == "output" && message["body"]["category"] == category
            })
            .map(|message| message["body"]["output"].as_str().unwrap())
            .collect::<String>()
            .replace("\r\n", "\n")
    }

    fn finish(mut self) {
        self.request("disconnect", json!({}));
        drop(self.stdin);
        self.child.wait().unwrap();
    }
}

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse().ok()?;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

#[test]
fn breakpoints_and_stepping() {
    let mut adapter = Adapter::start();
    // Line 6 is empty.
    let breakpoints = adapter.launch(&[8, 6, 4], &[], false);
    assert_eq!(breakpoints["breakpoints"][0]["verified"], false);

    assert_eq!(adapter.stopped("breakpoint"), ["run:8", "thunk:13"]);
    assert_eq!(adapter.locals(), ["x=4"]);

    adapter.step("stepIn");
    assert_eq!(adapter.stopped("step"), ["add:4", "run:8", "thunk:13"]);
    assert_eq!(adapter.locals(), ["a=4", "b=1"]);
    let evaluated = adapter.request("evaluate", json!({"expression": "(+ a b)", "frameId": 0}));
    assert_eq!(evaluated["result"], "5");

    adapter.step("next");
    assert_eq!(adapter.stopped("step"), ["add:5", "run:8", "thunk:13"]);
    assert_eq!(adapter.locals(), ["a=4", "b=1", "sum=5"]);

    adapter.step("stepOut");
    assert_eq!(adapter.stopped("step"), ["run:8", "thunk:13"]);
    adapter.step("next");
    assert_eq!(adapter.stopped("step"), ["run:9", "thunk:13"]);

    // `try` runs `add` in a child fiber.
    adapter.step("continue");
    assert_eq!(
        adapter.stopped("breakpoint"),
        ["add:4", "run:9", "thunk:13"]
    );
    assert_eq!(adapter.locals(), ["a=10", "b=0"]);
    adapter.step("stepOut");
    assert_eq!(adapter.stopped("step"), ["run:9", "thunk:13"]);

    adapter.step("continue");
    assert_eq!(adapter.event("exited")["exitCode"], 1);
    adapter.event("terminated");
    assert_eq!(adapter.output("stdout"), "checked 20\n");
    assert!(adapter.output("stderr").starts_with("error: boom"));

    // Only the breakpoints on code were verified.
    let mut verified: Vec<i64> = adapter
        .backlog
        .iter()
        .filter(|message| message["event"] == "breakpoint")
        .map(|message| message["body"]["breakpoint"]["line"].as_i64().unwrap())
        .collect();
    verified.sort_unstable();
    assert_eq!(verified, [4, 8]);
    adapter.finish();
}

#[test]
fn stops_on_entry_and_on_an_uncaught_error() {
    let mut adapter = Adapter::start();
    adapter.launch(&[], &["uncaught"], true);
    assert_eq!(adapter.stopped("entry"), ["thunk:3"]);
    adapter.step("next");
    assert_eq!(adapter.stopped("step"), ["thunk:7"]);

    adapter.step("continue");
    let stopped = adapter.event("stopped");
    assert_eq!(stopped["reason"], "exception");
    assert!(
        stopped["text"].as_str().unwrap().contains("boom"),
        "{stopped}"
    );
    let trace = adapter.request("stackTrace", json!({"threadId": 1}));
    assert_eq!(trace["stackFrames"][0]["line"], 15);

    adapter.step("continue");
    assert_eq!(adapter.event("exited")["exitCode"], 1);
    adapter.finish();
}

#[test]
fn stops_a_task() {
    let mut adapter = Adapter::debugging("tasks.janet");
    adapter.launch(&[4], &[], false);
    let stopped = adapter.event("stopped");
    assert_eq!(stopped["reason"], "breakpoint");
    assert_eq!(stopped["allThreadsStopped"], false);
    let thread = stopped["threadId"].as_i64().unwrap();
    assert_ne!(thread, 1);
    let threads = adapter.request("threads", json!({}));
    assert_eq!(
        threads["threads"],
        json!([{"id": 1, "name": "main"}, {"id": thread, "name": format!("task {thread}")}])
    );
    let trace = adapter.request("stackTrace", json!({"threadId": thread}));
    let frames: Vec<String> = trace["stackFrames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| format!("{}:{}", frame["name"].as_str().unwrap(), frame["line"]))
        .collect();
    assert_eq!(frames, ["work:4", "spawn:8"]);
    assert_eq!(adapter.locals(), ["x=5"]);

    adapter.request("next", json!({"threadId": thread}));
    assert_eq!(adapter.event("stopped")["threadId"], thread);
    assert_eq!(adapter.locals(), ["doubled=10", "x=5"]);

    adapter.request("continue", json!({"threadId": thread}));
    assert_eq!(
        adapter.event("thread"),
        json!({"reason": "exited", "threadId": thread})
    );
    assert_eq!(adapter.event("exited")["exitCode"], 0);
    assert_eq!(adapter.output("stdout"), "worked 10\ndone\n");
    let threads = adapter.request("threads", json!({}));
    assert_eq!(threads["threads"], json!([{"id": 1, "name": "main"}]));
    adapter.finish();
}

#[test]
fn attaches_to_the_repl() {
    // Below the ephemeral range: a port the OS just handed out may go to another test's listener.
    let port = 20_000 + u16::try_from(std::process::id() % 10_000).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // No token: any client is served, as by a netrepl the user started, which `port` attaches to.
    let mut repl = runtime
        .block_on(Netrepl::start("janet", port, Path::new("."), ""))
        .unwrap();

    let mut adapter = Adapter::start();
    let program = adapter.program.clone();
    adapter.request("initialize", json!({"adapterID": "Janet"}));
    adapter.request("attach", json!({"request": "attach", "port": port}));
    adapter.event("initialized");
    adapter.request(
        "setBreakpoints",
        json!({"source": {"path": program}, "breakpoints": [{"line": 4}]}),
    );
    // Zed sends the filter defaults; an attached REPL still does not stop on errors.
    adapter.request("setExceptionBreakpoints", json!({"filters": ["uncaught"]}));
    adapter.request("configurationDone", json!({}));

    // `add` as the kernel finds it in the fixture.
    let add = "(defn add [a b]\n  (def sum (+ a b))\n  (* sum 2))";
    let position = Position {
        path: program.into(),
        line: 3,
        column: 1,
    };
    let defined = runtime.block_on(repl.eval(add, Some(&position))).unwrap();
    assert_eq!(defined.errors, "");
    assert_eq!(adapter.event("breakpoint")["breakpoint"]["line"], 4);

    // The evaluation waits for the stop to be continued.
    let call = thread::spawn(move || {
        let evaluation = runtime.block_on(repl.eval("(add 1 2)", None)).unwrap();
        (runtime, repl, evaluation)
    });
    assert_eq!(adapter.stopped("breakpoint"), ["add:4", "thunk:1"]);
    assert_eq!(adapter.locals(), ["a=1", "b=2"]);
    adapter.step("continue");
    let (runtime, mut repl, evaluation) = call.join().unwrap();
    assert_eq!(evaluation.value, "6");

    let failed = runtime
        .block_on(repl.eval("(error \"oops\")", None))
        .unwrap();
    assert!(failed.errors.contains("oops"), "{failed:?}");

    // A busy evaluation pauses for the debugger, and the kernel's interrupt cancels it.
    #[cfg(unix)]
    let (runtime, mut repl) = {
        let pid = runtime.block_on(repl.pid()).unwrap();
        let busy = thread::spawn(move || {
            let evaluation = runtime.block_on(repl.eval("(while true)", None)).unwrap();
            (runtime, repl, evaluation)
        });
        thread::sleep(Duration::from_millis(500));
        adapter.request("pause", json!({"threadId": 1}));
        assert_eq!(adapter.stopped("pause"), ["thunk:1"]);
        adapter.step("continue");
        thread::sleep(Duration::from_millis(200));
        janet_lsp_plus::kernel::netrepl::signal(pid, "INT").unwrap();
        let (runtime, repl, evaluation) = busy.join().unwrap();
        assert!(evaluation.errors.contains("interrupted"), "{evaluation:?}");
        (runtime, repl)
    };

    adapter.finish();
    // The driver removes its hook once the adapter is gone.
    let hook = "(get (table/getproto (curenv)) :janet-zed/debugger)";
    let detached = (0..50).any(|_| {
        thread::sleep(Duration::from_millis(100));
        runtime.block_on(repl.call(hook)).unwrap() == "(true nil)"
    });
    assert!(detached);
    let again = runtime.block_on(repl.eval("(add 1 2)", None)).unwrap();
    assert_eq!(again.value, "6");
}

#[cfg(unix)]
#[test]
fn pauses_a_busy_program() {
    let mut adapter = Adapter::debugging("spin.janet");
    adapter.launch(&[], &[], false);
    thread::sleep(Duration::from_millis(500));
    adapter.request("pause", json!({"threadId": 1}));
    assert_eq!(adapter.stopped("pause"), ["spin:5", "thunk:7"]);
    let counted = adapter.request("evaluate", json!({"expression": "i", "frameId": 0}));
    assert_ne!(counted["result"], "0");

    // It runs on, and pauses again.
    adapter.step("continue");
    adapter.request("pause", json!({"threadId": 1}));
    assert_eq!(adapter.stopped("pause"), ["spin:5", "thunk:7"]);
    adapter.finish();
}
