//! What the shared netrepl knows about a name, for the language server: its handlers answer
//! synchronously, so over a blocking connection of its own. It only attaches: the server never
//! starts a REPL, nor loads a module into one (loading runs a module's side effects).

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::netrepl::{HOST, jdn_strings};

const LOOKUP: &str = include_str!("lookup.janet");
/// How a binding's declared types are written back, for `lookup.janet` to call.
const TYPES: &str = include_str!("../janet/types.janet");
const CONNECT_TIMEOUT: Duration = Duration::from_millis(50);
const READ_TIMEOUT: Duration = Duration::from_millis(300);

/// A binding in the REPL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The file, 1-based line and byte column of its definition.
    pub location: Option<(PathBuf, usize, usize)>,
    pub doc: Option<String>,
    /// `macro`, or the type of the value: `function`, `table`, …
    pub kind: String,
    /// The metadata struct its types were declared in, as Janet source: `{:ret :string}`.
    pub annotation: Option<String>,
}

pub struct Repl {
    stream: TcpStream,
}

impl Repl {
    pub fn attach(port: u16) -> io::Result<Self> {
        let address: SocketAddr = format!("{HOST}:{port}").parse().map_err(io::Error::other)?;
        let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        stream.set_write_timeout(Some(READ_TIMEOUT))?;
        let mut repl = Self { stream };
        // A plain first message is the client name; the server answers with a prompt.
        repl.send(b"janet-zed-lsp")?;
        repl.recv()?;
        Ok(repl)
    }

    /// The first of `candidates`, a module file and a name in it, that the REPL binds.
    pub fn lookup(&mut self, candidates: &[(PathBuf, String)]) -> io::Result<Option<Binding>> {
        let candidates = candidates
            .iter()
            .map(|(path, name)| {
                // A JSON string is a valid Janet string literal.
                Ok(format!(
                    "[{} {}]",
                    serde_json::to_string(&path.to_string_lossy())?,
                    serde_json::to_string(name)?
                ))
            })
            .collect::<Result<Vec<_>, serde_json::Error>>()?
            .join(" ");
        // The helpers are defined inside the form: the REPL's own environment stays as it was.
        let code = format!("(do {TYPES} ({LOOKUP} [{candidates}]))");
        self.send(&[&[0xFF], code.as_bytes()].concat())?;
        let reply = String::from_utf8_lossy(&self.recv()?).into_owned();
        Ok(parse_reply(&reply))
    }

    /// netrepl framing: 4-byte little-endian length, then the payload.
    fn send(&mut self, payload: &[u8]) -> io::Result<()> {
        let len = u32::try_from(payload.len()).map_err(io::Error::other)?;
        self.stream.write_all(&len.to_le_bytes())?;
        self.stream.write_all(payload)
    }

    fn recv(&mut self) -> io::Result<Vec<u8>> {
        let mut len = [0; 4];
        self.stream.read_exact(&mut len)?;
        let mut payload = vec![0; u32::from_le_bytes(len) as usize];
        self.stream.read_exact(&mut payload)?;
        Ok(payload)
    }
}

/// `(true ("cwd" "source" "line" "column" "doc" "type" "macro" "types"))`, `(true ())` when
/// nothing is bound.
fn parse_reply(reply: &str) -> Option<Binding> {
    if !reply.starts_with("(true") {
        return None;
    }
    let [cwd, source, line, column, doc, kind, is_macro, types] =
        jdn_strings(reply).try_into().ok()?;
    // Code typed into the REPL has no file: its source is `:zed`.
    let file = Path::new(&cwd).join(source);
    let location = match (line.parse(), column.parse()) {
        (Ok(line), Ok(column)) if file.is_file() => Some((file, line, column)),
        _ => None,
    };
    Some(Binding {
        location,
        doc: Some(doc).filter(|doc| !doc.is_empty()),
        kind: if is_macro.is_empty() { kind } else { is_macro },
        annotation: Some(types).filter(|types| !types.is_empty()),
    })
}

#[cfg(test)]
mod tests;
