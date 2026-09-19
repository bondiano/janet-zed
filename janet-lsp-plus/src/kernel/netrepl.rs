//! Client for a spork/netrepl server: the shared Janet process behind Zed's REPL and terminal clients.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};

pub const HOST: &str = "127.0.0.1";
/// Under the user's home: the port and token files of the REPL kernels, one pair per project.
const PORTS: &str = ".cache/janet-zed/repl";
/// Bound in a kernel's netrepl server to the project it serves, so that a port file left behind
/// by a dead kernel does not lead to another project's REPL once that one takes the port.
pub const PROJECT: &str = "janet-zed/project";

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
    /// Starts a netrepl server with `janet` on `port` and connects to it. Never to a server
    /// already listening there: whoever holds the port would see every evaluation. The server
    /// serves only clients named with `token`.
    pub async fn start(janet: &str, port: u16, project: &Path, token: &str) -> io::Result<Self> {
        drop(std::net::TcpListener::bind((HOST, port))?);
        let (stream, server) = start_server(janet, port, project, token).await?;
        Self::handshake(stream, Some(server), &format!("{token}zed")).await
    }

    /// Connects to the REPL a kernel recorded for `project`, with the token it recorded.
    pub async fn attach_recorded(project: &Path) -> io::Result<Self> {
        let (port, token) = recorded(project)
            .ok_or_else(|| io::Error::other("no REPL kernel recorded for this project"))?;
        let mut repl = Self::attach(&format!("{HOST}:{port}"), &format!("{token}zed")).await?;
        let reply = repl.call(PROJECT).await?;
        if !serves(&reply, project) {
            return Err(io::Error::other(format!(
                "the REPL on port {port} is not this project's"
            )));
        }
        Ok(repl)
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
        // A plain first message is the client name, never empty; the server answers with a prompt,
        // or a kernel's server closes the connection when the name lacks its token.
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

/// A port nobody listens on. Another process may take it before the server binds it, which
/// makes the server fail to start rather than connect anywhere else.
pub fn free_port() -> io::Result<u16> {
    Ok(std::net::TcpListener::bind((HOST, 0))?.local_addr()?.port())
}

/// The project a directory belongs to, for its REPL: the nearest directory holding it with a
/// `.git` or a `project.janet`, else the directory itself. A kernel starts in the directory of
/// the file it runs, the language server and the terminal task in the worktree root.
// ponytail: a worktree without either marker gets one REPL per directory its files are in.
pub fn project_of(dir: &Path) -> PathBuf {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    dir.ancestors()
        .find(|ancestor| ancestor.join(".git").exists() || ancestor.join("project.janet").is_file())
        .map_or_else(|| dir.clone(), Path::to_path_buf)
}

/// A fresh secret for a REPL server: whoever reads it may run code there.
pub fn new_token() -> io::Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(u128::from_le_bytes(bytes).to_string())
}

/// Where the REPL of `project` keeps its `port` and `token` files, under `ports`: at the
/// project's own path, so that the terminal task finds them from `$ZED_WORKTREE_ROOT` alone.
fn record_dir_under(ports: &Path, project: &Path) -> PathBuf {
    // On Windows `C:\x` becomes `C\x`, relative, as the terminal task spells it too.
    let project = project.to_string_lossy().replace(':', "");
    ports.join(project.trim_start_matches(['/', '\\']))
}

/// The port and token the REPL of `project` recorded, if one has. It may be stale: check what
/// answers.
pub fn recorded(project: &Path) -> Option<(u16, String)> {
    read_record(&record_dir_under(&ports_dir()?, project))
}

/// The token `project`'s kernel recorded, when it recorded `port`: a client attaching to that
/// port by number still gets in, and a server on any other port never sees the token.
pub fn token_for(project: &Path, port: u16) -> Option<String> {
    recorded(project).and_then(|(recorded, token)| (recorded == port).then_some(token))
}

fn read_record(dir: &Path) -> Option<(u16, String)> {
    let read = |name| std::fs::read_to_string(dir.join(name)).ok();
    let port = read("port")?.trim().parse().ok()?;
    Some((port, read("token")?.trim().to_string()))
}

/// Records `port` and `token` as the REPL of `project`, readable by this user only.
pub fn record(project: &Path, port: u16, token: &str) -> io::Result<()> {
    let ports = ports_dir().ok_or_else(|| io::Error::other("no home directory"))?;
    write_record(&ports, &record_dir_under(&ports, project), port, token)
}

fn write_record(ports: &Path, dir: &Path, port: u16, token: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    private_dir(ports)?;
    // The token first: a reader that finds the new port finds its token.
    write_private(&dir.join("token"), token)?;
    write_private(&dir.join("port"), &port.to_string())
}

/// Lets only this user into `dir`.
#[cfg(unix)]
fn private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// The profile's permissions apply, see [`ports_dir`].
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn private_dir(_: &Path) -> io::Result<()> {
    Ok(())
}

/// Replaces `file` with `text`, only this user may read, at once: a reader sees the old text or
/// the new, and kernels starting together each write a file of their own. On Windows the
/// profile's permissions apply, see [`ports_dir`].
fn write_private(file: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    static WRITTEN: AtomicU64 = AtomicU64::new(0);
    let written = WRITTEN.fetch_add(1, Ordering::Relaxed);
    let temporary = file.with_extension(format!("{}-{written}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(&temporary)
        .and_then(|mut open| open.write_all(text.as_bytes()))
        .and_then(|()| std::fs::rename(&temporary, file))
        .inspect_err(|_| {
            std::fs::remove_file(&temporary).ok();
        })
}

/// Whether a REPL's `reply` to [`PROJECT`] names `project`.
pub fn serves(reply: &str, project: &Path) -> bool {
    reply.starts_with("(true")
        && jdn_strings(reply)
            .first()
            .is_some_and(|served| Path::new(served) == project)
}

/// On Windows under the user profile, whose ACL admits only its owner (and SYSTEM and
/// administrators), in place of the modes set on Unix. Never under a `HOME` a shell may point elsewhere.
fn ports_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(PORTS))
}

/// Serves netrepl on `port` until the kernel's end of stdin closes, so a killed kernel leaves no orphan.
/// A client whose name does not start with `token` is disconnected before its first form
/// (netrepl makes a taken name unique by appending to it). The token comes first on stdin, not
/// on the command line other users can list; stderr, where netrepl logs every name, is dropped.
async fn start_server(
    janet: &str,
    port: u16,
    project: &Path,
    token: &str,
) -> io::Result<(TcpStream, Child)> {
    // A JSON string is a valid Janet string literal.
    let project = serde_json::to_string(&project.to_string_lossy())?;
    let serve = format!(
        "(import spork/netrepl)
(def {PROJECT} {project})
(def token (string/trim (file/read stdin :line)))
(ev/thread (fn [] (file/read stdin :all) (os/exit 0)) nil :n)
(def env (make-env))
(put env :pretty-format \"%.20Q\")
(defn env-of [name stream]
  (if (and (bytes? name) (string/has-prefix? token name))
    env
    (do (:close stream) @{{}})))
(netrepl/run-server \"{HOST}\" \"{port}\" env-of)"
    );
    let mut server = Command::new(janet)
        .args(["-e", &serve])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    server
        .stdin
        .as_mut()
        .ok_or_else(|| io::Error::other("no stdin for the netrepl server"))?
        .write_all(format!("{token}\n").as_bytes())
        .await?;
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
pub(super) fn jdn_strings(jdn: &str) -> Vec<String> {
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
