//! The language server end to end, over an in-memory connection, on `fixtures/project`.
//! Needs `janet` on PATH, like the server itself.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};
use url::Url;

/// A server with `src/report.janet` of the fixture project open.
struct Session {
    connection: Connection,
    server: JoinHandle<()>,
    next_id: i32,
    notifications: Vec<Notification>,
    root: PathBuf,
    /// The text of `src/report.janet`.
    text: String,
    /// The URI of `src/report.janet`.
    report: String,
}

impl Session {
    fn start() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/project")
            .canonicalize()
            .unwrap();
        let (server, connection) = Connection::memory();
        let server =
            thread::spawn(move || janet_zed_server::lsp::run_with(&server, false).unwrap());
        let text = std::fs::read_to_string(root.join("src/report.janet")).unwrap();
        let report = uri(&root.join("src/report.janet"));
        let mut session = Self {
            connection,
            server,
            next_id: 0,
            notifications: Vec::new(),
            root,
            text,
            report,
        };

        let root_uri = Url::from_directory_path(&session.root).unwrap().to_string();
        session
            .request(
                "initialize",
                json!({
                    "capabilities": {},
                    "workspaceFolders": [{"uri": root_uri, "name": "project"}],
                    "initializationOptions": {"janetPath": "janet"},
                }),
            )
            .unwrap();
        session.notify("initialized", json!({}));
        session.open(&session.report.clone(), &session.text.clone());
        session
    }

    fn finish(mut self) {
        self.request("shutdown", Value::Null).unwrap();
        self.notify("exit", Value::Null);
        self.server.join().unwrap();
    }

    fn open(&self, uri: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "janet", "version": 1, "text": text}}),
        );
    }

    /// Position params `delta` bytes past the first `needle` in `src/report.janet`.
    fn at(&self, needle: &str, delta: usize) -> Value {
        json!({
            "textDocument": {"uri": self.report},
            "position": position(&self.text, needle, delta),
        })
    }

    /// The line of `src/report.janet` with the first `needle`, and `↑` under the byte `delta`
    /// past it.
    fn cursor(&self, needle: &str, delta: usize) -> String {
        let offset = self.text.find(needle).unwrap() + delta;
        let start = self.text[..offset].rfind('\n').map_or(0, |index| index + 1);
        let end = self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |index| offset + index);
        format!(
            "{}\n{}↑",
            &self.text[start..end],
            " ".repeat(offset - start)
        )
    }

    /// `path line:character-line:character`, with the path relative to the fixture project.
    fn show_location(&self, uri: &str, range: &Value) -> String {
        let root = Url::from_directory_path(&self.root).unwrap().to_string();
        let at = |position: &Value| format!("{}:{}", position["line"], position["character"]);
        format!(
            "{} {}-{}",
            uri.strip_prefix(&root).unwrap(),
            at(&range["start"]),
            at(&range["end"])
        )
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request::new(id.clone(), method.to_string(), params);
        self.connection.sender.send(request.into()).unwrap();
        loop {
            match self.receive() {
                Message::Response(response) if response.id == id => {
                    return response.response_result.map_err(|error| error.message);
                }
                Message::Notification(notification) => self.notifications.push(notification),
                // Requests from the server, like capability registration.
                _ => {}
            }
        }
    }

    fn notify(&self, method: &str, params: Value) {
        let notification = Notification::new(method.to_string(), params);
        self.connection.sender.send(notification.into()).unwrap();
    }

    /// The diagnostics published for `uri` at `version`.
    fn diagnostics(&mut self, uri: &str, version: i64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let published = self.notifications.iter().position(|notification| {
                notification.method == "textDocument/publishDiagnostics"
                    && notification.params["uri"] == uri
                    && notification.params["version"] == version
            });
            if let Some(index) = published {
                return self.notifications.remove(index).params["diagnostics"].clone();
            }
            assert!(
                Instant::now() < deadline,
                "no diagnostics for {uri} v{version}"
            );
            if let Message::Notification(notification) = self.receive() {
                self.notifications.push(notification);
            }
        }
    }

    fn receive(&self) -> Message {
        self.connection
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
    }
}

/// The LSP position `delta` bytes past the first `needle` in ASCII `text`.
fn position(text: &str, needle: &str, delta: usize) -> Value {
    let offset = text.find(needle).unwrap() + delta;
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    json!({"line": text[..offset].matches('\n').count(), "character": offset - line_start})
}

fn uri(path: &Path) -> String {
    Url::from_file_path(path).unwrap().to_string()
}

/// Document symbols as an indented outline.
fn outline(symbols: &Value, depth: usize) -> Vec<String> {
    symbols
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|symbol| {
            let name = format!("{}{}", "  ".repeat(depth), symbol["name"].as_str().unwrap());
            std::iter::once(name).chain(outline(&symbol["children"], depth + 1))
        })
        .collect()
}

#[test]
fn hover_on_an_imported_definition() {
    let mut session = Session::start();
    let hover = session
        .request("textDocument/hover", session.at("shapes/area", 8))
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- HOVER\n{}\n",
        session.cursor("shapes/area", 8),
        hover["contents"]["value"].as_str().unwrap()
    ));
    session.finish();
}

#[test]
fn hover_on_peg_specials() {
    let mut session = Session::start();
    // `some` is a core function too; in the pattern it is the PEG special.
    let source = "(peg/match ~(some (constant :x)) \"a\")\n";
    let scratch = uri(&session.root.join("scratch.janet"));
    session.open(&scratch, source);
    let hovers: Vec<_> = ["some", "constant"]
        .iter()
        .map(|name| {
            let params = json!({
                "textDocument": {"uri": scratch},
                "position": position(source, name, 0),
            });
            let hover = session.request("textDocument/hover", params).unwrap();
            format!(
                "----- HOVER {name}\n{}",
                hover["contents"]["value"].as_str().unwrap()
            )
        })
        .collect();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n{}\n",
        hovers.join("\n\n")
    ));
    session.finish();
}

#[test]
fn completion_in_a_call() {
    let mut session = Session::start();
    let items = session
        .request(
            "textDocument/completion",
            session.at("(string/join lines", 1),
        )
        .unwrap();
    // Core bindings come from the installed Janet, so only their presence is checked.
    let (core, found): (Vec<_>, Vec<_>) = items
        .as_array()
        .unwrap()
        .iter()
        .partition(|item| item["data"]["source"] == "core");
    assert!(core.iter().any(|item| item["label"] == "string/join"));
    // Sorted: definitions come in hash order within a module; ranking is a unit test's business.
    let mut found: Vec<_> = found
        .iter()
        .map(|item| {
            format!(
                "{} {}",
                item["label"].as_str().unwrap(),
                item["detail"].as_str().unwrap_or_default()
            )
        })
        .collect();
    found.sort();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- COMPLETIONS\n{}\n",
        session.cursor("(string/join lines", 1),
        found.join("\n")
    ));
    session.finish();
}

#[test]
fn signature_help_at_the_second_argument() {
    let mut session = Session::start();
    let help = session
        .request(
            "textDocument/signatureHelp",
            session.at("(string/join lines \"", 19),
        )
        .unwrap();
    let signature = &help["signatures"][0];
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- SIGNATURE\n{}\nactive parameter: {}\n",
        session.cursor("(string/join lines \"", 19),
        signature["label"].as_str().unwrap(),
        signature["activeParameter"]
    ));
    session.finish();
}

#[test]
fn definition_of_a_local() {
    let mut session = Session::start();
    let location = session
        .request("textDocument/definition", session.at("(keys by-kind)", 8))
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- DEFINITION\n{}\n",
        session.cursor("(keys by-kind)", 8),
        session.show_location(location["uri"].as_str().unwrap(), &location["range"])
    ));
    session.finish();
}

#[test]
fn definition_of_an_imported_module() {
    let mut session = Session::start();
    let location = session
        .request(
            "textDocument/definition",
            session.at("(import ./shapes", 10),
        )
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- DEFINITION\n{}\n",
        session.cursor("(import ./shapes", 10),
        session.show_location(location["uri"].as_str().unwrap(), &location["range"])
    ));
    session.finish();
}

#[test]
fn references_with_the_declaration() {
    let mut session = Session::start();
    let mut params = session.at("shapes/area", 8);
    params["context"] = json!({"includeDeclaration": true});
    let references = session.request("textDocument/references", params).unwrap();
    let mut locations: Vec<_> = references
        .as_array()
        .unwrap()
        .iter()
        .map(|location| {
            session.show_location(location["uri"].as_str().unwrap(), &location["range"])
        })
        .collect();
    locations.sort();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- REFERENCES\n{}\n",
        session.cursor("shapes/area", 8),
        locations.join("\n")
    ));
    session.finish();
}

#[test]
fn rename_an_imported_definition() {
    let mut session = Session::start();
    let mut params = session.at("shapes/area", 8);
    params["newName"] = json!("area-of");
    let rename = session.request("textDocument/rename", params).unwrap();
    let mut edits: Vec<_> = rename["changes"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(uri, edits)| {
            edits.as_array().unwrap().iter().map(|edit| {
                format!(
                    "{} {}",
                    session.show_location(uri, &edit["range"]),
                    edit["newText"].as_str().unwrap()
                )
            })
        })
        .collect();
    edits.sort();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- EDITS\n{}\n",
        session.cursor("shapes/area", 8),
        edits.join("\n")
    ));
    session.finish();
}

#[test]
fn rename_a_core_binding() {
    let mut session = Session::start();
    let mut params = session.at("(map", 2);
    params["newName"] = json!("m");
    let refused = session.request("textDocument/rename", params).unwrap_err();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- ERROR\n{refused}\n",
        session.cursor("(map", 2)
    ));
    session.finish();
}

#[test]
fn document_symbols() {
    let mut session = Session::start();
    let symbols = session
        .request(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": session.report}}),
        )
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- SYMBOLS\n{}\n",
        outline(&symbols, 0).join("\n")
    ));
    session.finish();
}

#[test]
fn code_actions_at_a_threading_macro() {
    let mut session = Session::start();
    let cursor = position(&session.text, "(->> items", 0);
    let actions = session
        .request(
            "textDocument/codeAction",
            json!({
                "textDocument": {"uri": session.report},
                "range": {"start": cursor, "end": cursor},
                "context": {"diagnostics": []},
            }),
        )
        .unwrap();
    let titles: Vec<_> = actions
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action["title"].as_str().unwrap())
        .collect();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- ACTIONS\n{}\n",
        session.cursor("(->> items", 0),
        titles.join("\n")
    ));
    session.finish();
}

#[test]
fn quick_fix_for_an_unknown_symbol() {
    let mut session = Session::start();
    let report = session.report.clone();
    let broken = session.text.replace("(length items)", "(lenght items)");
    session.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": report, "version": 2}, "contentChanges": [{"text": broken}]}),
    );
    session.diagnostics(&report, 2);
    let cursor = position(&broken, "(lenght", 1);
    let actions = session
        .request(
            "textDocument/codeAction",
            json!({
                "textDocument": {"uri": report},
                "range": {"start": cursor, "end": cursor},
                "context": {"diagnostics": []},
            }),
        )
        .unwrap();
    let actions: Vec<_> = actions
        .as_array()
        .unwrap()
        .iter()
        .map(|action| {
            format!(
                "{} ({})",
                action["title"].as_str().unwrap(),
                action["kind"].as_str().unwrap()
            )
        })
        .collect();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n(lenght items)\n ↑\n\n----- ACTIONS\n{}\n",
        actions.join("\n")
    ));
    session.finish();
}

#[test]
fn vocabulary_of_project_janet() {
    let mut session = Session::start();
    let project = uri(&session.root.join("project.janet"));
    let text = std::fs::read_to_string(session.root.join("project.janet")).unwrap();
    // Checked against the installed tools' bindings: no unknown symbols, and `post-deps` does not
    // load the dependency while expanding.
    let source =
        format!("{text}\n(task \"check\" [] (run-tests))\n(post-deps (import not-installed))\n");
    session.open(&project, &source);
    assert_eq!(session.diagnostics(&project, 1), json!([]));

    let at = |needle: &str, delta: usize| {
        json!({
            "textDocument": {"uri": project},
            "position": position(&source, needle, delta),
        })
    };
    // Its kind line names the installed tool, so only the docs are checked.
    let hover = session
        .request("textDocument/hover", at("declare-source", 0))
        .unwrap();
    let hover = hover["contents"]["value"].as_str().unwrap();
    assert!(hover.starts_with("```janet\n(declare-source &named source prefix)\n```"));
    assert!(hover.contains("- `:prefix`: "));

    let help = session
        .request("textDocument/signatureHelp", at("\"src/shapes", 1))
        .unwrap();
    let signature = &help["signatures"][0];
    insta::assert_snapshot!(format!(
        "----- SIGNATURE at :source\n{}\nactive parameter: {}\n",
        signature["label"].as_str().unwrap(),
        signature["activeParameter"]
    ));

    // jpm comes with Homebrew's `janet`, which the tests run.
    let definition = session
        .request("textDocument/definition", at("declare-source", 0))
        .unwrap();
    let target = definition["uri"].as_str().unwrap();
    assert!(
        target.ends_with("/jpm/declare.janet") || target.ends_with("/spork/declare-cc.janet"),
        "{target}"
    );

    let project_labels = |items: &Value| -> Vec<String> {
        items
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["data"]["source"] == "project")
            .map(|item| item["label"].as_str().unwrap().to_string())
            .collect()
    };
    let in_project = session
        .request("textDocument/completion", at("declare-source", 1))
        .unwrap();
    assert!(project_labels(&in_project).contains(&"declare-native".to_string()));
    let in_module = session
        .request(
            "textDocument/completion",
            session.at("(string/join lines", 1),
        )
        .unwrap();
    assert!(project_labels(&in_module).is_empty());
    session.finish();
}

#[test]
fn no_diagnostics_for_a_valid_file() {
    let mut session = Session::start();
    let report = session.report.clone();
    assert_eq!(session.diagnostics(&report, 1), json!([]));
    session.finish();
}

#[test]
fn diagnostics_after_an_edit() {
    let mut session = Session::start();
    let report = session.report.clone();
    let broken = session.text.replace("(length items)", "(lenght items)");
    session.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": report, "version": 2}, "contentChanges": [{"text": broken}]}),
    );
    let problems: Vec<_> = session
        .diagnostics(&report, 2)
        .as_array()
        .unwrap()
        .iter()
        .map(|problem| {
            format!(
                "{} {}",
                session.show_location(&report, &problem["range"]),
                problem["message"].as_str().unwrap()
            )
        })
        .collect();
    insta::assert_snapshot!(format!(
        "----- CHANGE\n(length items) -> (lenght items)\n\n----- DIAGNOSTICS\n{}\n",
        problems.join("\n")
    ));
    session.finish();
}

#[test]
fn formats_a_buffer_not_on_disk() {
    let mut session = Session::start();
    let source = "(defn f [x]\n(+ x\n1))\n";
    let scratch = uri(&session.root.join("scratch.janet"));
    session.open(&scratch, source);
    let edits = session
        .request(
            "textDocument/formatting",
            json!({"textDocument": {"uri": scratch}, "options": {"tabSize": 2, "insertSpaces": true}}),
        )
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- FORMATTED\n{}",
        edits[0]["newText"].as_str().unwrap()
    ));
    session.finish();
}
