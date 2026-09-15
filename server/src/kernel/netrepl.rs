//! Client for a spork/netrepl server: the shared Janet process behind Zed's REPL and terminal clients.

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};

// ponytail: one fixed port, so every Zed window shares one Janet process; derive it per worktree if that bites.
pub const HOST: &str = "127.0.0.1";
pub const PORT: u16 = 9365;

const EVAL: &str = include_str!("eval.janet");

#[derive(Debug, Default, PartialEq)]
pub struct Evaluation {
    pub value: String,
    pub output: String,
    pub errors: String,
}

/// Where editor code sits in its file: the 1-based line and byte column of its first character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

pub struct Netrepl {
    stream: TcpStream,
    _server: Option<Child>,
}

impl Netrepl {
    /// Attaches to a netrepl server already listening on `port`, or starts one with `janet`.
    pub async fn connect(janet: &str, port: u16) -> io::Result<Self> {
        if let Ok(stream) = TcpStream::connect((HOST, port)).await {
            return Self::handshake(stream, None, "zed").await;
        }
        let (stream, server) = start_server(janet, port).await?;
        Self::handshake(stream, Some(server), "zed").await
    }

    /// Connects as client `name` to a netrepl server listening on `address`.
    pub async fn attach(address: &str, name: &str) -> io::Result<Self> {
        Self::handshake(TcpStream::connect(address).await?, None, name).await
    }

    async fn handshake(stream: TcpStream, server: Option<Child>, name: &str) -> io::Result<Self> {
        let mut repl = Self {
            stream,
            _server: server,
        };
        // A plain first message is the client name; the server answers with a prompt.
        repl.send(name.as_bytes()).await?;
        repl.recv().await?;
        Ok(repl)
    }

    /// Evaluates `code` in the shared env, compiled at `position` when the kernel found it in a file.
    pub async fn eval(
        &mut self,
        code: &str,
        position: Option<&Position>,
    ) -> io::Result<Evaluation> {
        // A JSON string is a valid Janet string literal.
        let code = serde_json::to_string(code)?;
        let (source, line, column) = match position {
            Some(position) => (
                serde_json::to_string(&position.path.to_string_lossy())?,
                position.line,
                position.column,
            ),
            None => (":zed".to_string(), 1, 1),
        };
        let reply = self
            .call(&format!("({EVAL} {code} {source} {line} {column})"))
            .await?;
        Ok(parse_reply(&reply))
    }

    /// Evaluates `form` through netrepl's 0xFF channel and returns the JDN reply.
    pub async fn call(&mut self, form: &str) -> io::Result<String> {
        self.send(&[&[0xFF], form.as_bytes()].concat()).await?;
        Ok(String::from_utf8_lossy(&self.recv().await?).into_owned())
    }

    /// netrepl framing: 4-byte little-endian length, then the payload.
    async fn send(&mut self, payload: &[u8]) -> io::Result<()> {
        let len = u32::try_from(payload.len()).map_err(io::Error::other)?;
        self.stream.write_all(&len.to_le_bytes()).await?;
        self.stream.write_all(payload).await
    }

    async fn recv(&mut self) -> io::Result<Vec<u8>> {
        let mut len = [0; 4];
        self.stream.read_exact(&mut len).await?;
        let mut payload = vec![0; u32::from_le_bytes(len) as usize];
        self.stream.read_exact(&mut payload).await?;
        Ok(payload)
    }
}

/// Serves netrepl on `port` until the kernel's end of stdin closes, so a killed kernel leaves no orphan.
async fn start_server(janet: &str, port: u16) -> io::Result<(TcpStream, Child)> {
    let serve = format!(
        "(import spork/netrepl)
(ev/thread (fn [] (file/read stdin :all) (os/exit 0)) nil :n)
(netrepl/run-server-single \"{HOST}\" \"{port}\")"
    );
    let mut server = Command::new(janet)
        .args(["-e", &serve])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    for _ in 0..50 {
        if let Some(status) = server.try_wait()? {
            return Err(io::Error::other(format!(
                "`{janet}` netrepl server exited ({status}); is spork installed? (jpm install spork)"
            )));
        }
        if let Ok(stream) = TcpStream::connect((HOST, port)).await {
            return Ok((stream, server));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(io::Error::other(format!(
        "netrepl server did not listen on {HOST}:{port}"
    )))
}

/// netrepl answers `(protect (eval …))` as JDN: `(true ("value" "output" "errors"))` or `(false "message")`.
fn parse_reply(reply: &str) -> Evaluation {
    match (reply.starts_with("(true"), jdn_strings(reply).as_slice()) {
        (true, [value, output, errors]) => Evaluation {
            value: value.clone(),
            output: output.clone(),
            errors: errors.clone(),
        },
        _ => Evaluation {
            errors: format!("unexpected netrepl reply: {reply}"),
            ..Evaluation::default()
        },
    }
}

/// Decodes every string literal in a JDN text, in order.
fn jdn_strings(jdn: &str) -> Vec<String> {
    let mut bytes = jdn.bytes();
    let mut strings = Vec::new();
    while bytes.any(|b| b == b'"') {
        let mut decoded = Vec::new();
        while let Some(b) = bytes.next() {
            match b {
                b'"' => break,
                b'\\' => match bytes.next() {
                    Some(b'0') => decoded.push(0),
                    Some(b'a') => decoded.push(0x07),
                    Some(b'b') => decoded.push(0x08),
                    Some(b't') => decoded.push(b'\t'),
                    Some(b'n') => decoded.push(b'\n'),
                    Some(b'v') => decoded.push(0x0B),
                    Some(b'f') => decoded.push(0x0C),
                    Some(b'r') => decoded.push(b'\r'),
                    Some(b'e') => decoded.push(0x1B),
                    Some(b'x') => {
                        let hex = [bytes.next(), bytes.next()];
                        if let [Some(hi), Some(lo)] = hex
                            && let Ok(byte) =
                                u8::from_str_radix(&String::from_utf8_lossy(&[hi, lo]), 16)
                        {
                            decoded.push(byte);
                        }
                    }
                    Some(other) => decoded.push(other),
                    None => break,
                },
                _ => decoded.push(b),
            }
        }
        strings.push(String::from_utf8_lossy(&decoded).into_owned());
    }
    strings
}

#[cfg(test)]
mod tests;
