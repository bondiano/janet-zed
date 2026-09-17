//! The language server end to end, over an in-memory connection, on `fixtures/project`.
//! Needs `janet` on PATH, like the server itself.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use janet_zed_server::kernel::netrepl::{Netrepl, Position};
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
        Self::start_at(Self::root(), "src/report.janet")
    }

    /// The fixture project a session opens by default.
    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/project")
            .canonicalize()
            .unwrap()
    }

    /// A session that asks the REPL on `port`: the one this test started, never another test's.
    fn start_with_repl(port: u16) -> Self {
        Self::start_with(Self::root(), "src/report.janet", &json!({"replPort": port}))
    }

    /// A server on `root` with one file of it open.
    fn start_at(root: PathBuf, open: &str) -> Self {
        Self::start_with(root, open, &Value::Null)
    }

    /// The same, with `settings` merged into the `initializationOptions`.
    fn start_with(root: PathBuf, open: &str, settings: &Value) -> Self {
        let (server, connection) = Connection::memory();
        let server =
            thread::spawn(move || janet_zed_server::lsp::run_with(&server, false).unwrap());
        let text = std::fs::read_to_string(root.join(open)).unwrap();
        let report = uri(&root.join(open));
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
                    "initializationOptions": options(settings),
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

    /// Opens another file of the project as it is on disk.
    fn open_file(&mut self, relative: &str) -> (String, String) {
        let path = self.root.join(relative);
        let text = std::fs::read_to_string(&path).unwrap();
        let uri = uri(&path);
        self.open(&uri, &text);
        (uri, text)
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

/// `initializationOptions` with the keys of `settings` on top.
fn options(settings: &Value) -> Value {
    let mut options = json!({"janetPath": "janet", "replPort": repl_port(0)});
    for (key, value) in settings.as_object().into_iter().flatten() {
        options[key] = value.clone();
    }
    options
}

/// The LSP position `delta` bytes past the first `needle` in ASCII `text`.
fn position(text: &str, needle: &str, delta: usize) -> Value {
    let offset = text.find(needle).unwrap() + delta;
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    json!({"line": text[..offset].matches('\n').count(), "character": offset - line_start})
}

/// The REPL port sessions ask: not the kernel's, which a REPL of the developer's may hold. Below
/// the ephemeral range, apart from the other tests' ports. `slot` keeps the tests that start a
/// Janet process off each other's port: they run in parallel, and the process dies with the
/// `Netrepl` that spawned it, so a shared port lets the first to finish reset the other's REPL.
fn repl_port(slot: u16) -> u16 {
    40_000 + u16::try_from(std::process::id() % 8_000).unwrap() * 3 + slot
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

/// An edit in another buffer is what hover on an imported name answers from, saved or not.
#[test]
fn an_unsaved_edit_retypes_what_imports_it() {
    let mut session = Session::start();
    let hover = |session: &mut Session| {
        let hover = session
            .request("textDocument/hover", session.at("shapes/area", 8))
            .unwrap();
        hover["contents"]["value"].as_str().unwrap().to_string()
    };
    let before = hover(&mut session);
    let (shapes, text) = session.open_file("src/shapes.janet");
    // Without its metadata, `area` is whatever its body says it is.
    let annotation = "  {:params [Shape] :ret :number :throws [:string]}\n";
    let rect = "(* (shape :w) (shape :h))";
    assert!(text.contains(annotation) && text.contains(rect), "{text}");
    let edited = text.replace(annotation, "").replace(rect, "\"flat\"");
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": shapes, "version": 2},
            "contentChanges": [{"text": edited}],
        }),
    );
    let after = hover(&mut session);
    insta::assert_snapshot!(format!("----- BEFORE\n{before}\n\n----- AFTER\n{after}\n"));
    session.finish();
}

#[test]
fn hover_on_declared_types() {
    let mut session = Session::start();
    let path = session.root.join("src/shapes.janet");
    let text = std::fs::read_to_string(&path).unwrap();
    let shapes = uri(&path);
    session.open(&shapes, &text);
    let mut hover = |needle: &str, delta: usize| {
        let params = json!({
            "textDocument": {"uri": shapes},
            "position": position(&text, needle, delta),
        });
        let hover = session.request("textDocument/hover", params).unwrap();
        hover["contents"]["value"].as_str().unwrap().to_string()
    };
    let shown = [("defn circle", 5), ("def Shape", 4), ("defmacro timed", 9)]
        .map(|(needle, delta)| format!("-- {needle}\n{}", hover(needle, delta)));
    insta::assert_snapshot!(format!("----- HOVER\n{}\n", shown.join("\n\n")));
    session.finish();
}

#[test]
fn signature_help_names_the_declared_parameter_type() {
    let mut session = Session::start();
    let help = session
        .request(
            "textDocument/signatureHelp",
            session.at("(shapes/area s", 13),
        )
        .unwrap();
    let signature = &help["signatures"][0];
    let parameters = signature["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|parameter| {
            let label = parameter["label"].as_array().unwrap();
            let at = |index: usize| usize::try_from(label[index].as_u64().unwrap()).unwrap();
            let text = signature["label"].as_str().unwrap();
            text[at(0)..at(1)].to_string()
        })
        .collect::<Vec<_>>();
    insta::assert_snapshot!(format!(
        "----- CURSOR\n{}\n\n----- SIGNATURE\n{}\nparameters: {}\nactive parameter: {}\n",
        session.cursor("(shapes/area s", 13),
        signature["label"].as_str().unwrap(),
        parameters.join(", "),
        signature["activeParameter"]
    ));
    session.finish();
}

/// A local holding a function: nobody wrote its parameters down, so the help is the types it
/// takes, read off the imported definition it was bound to.
#[test]
fn signature_help_for_a_local_holding_a_function() {
    let mut session = Session::start();
    let source =
        "(import ./shapes)\n\n(defn plot []\n  (let [make shapes/circle]\n    (make 1)))\n";
    let scratch = uri(&session.root.join("src/scratch.janet"));
    session.open(&scratch, source);
    let params = json!({
        "textDocument": {"uri": scratch},
        "position": position(source, "(make 1)", 6),
    });
    let help = session
        .request("textDocument/signatureHelp", params)
        .unwrap();
    let label = help["signatures"][0]["label"].as_str().unwrap().to_string();
    session.finish();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- SIGNATURE\n{label}\n"
    ));
}

#[test]
fn hover_and_definition_of_an_ambient_declaration() {
    let mut session = Session::start();
    let (people, text) = session.open_file("src/people.janet");
    let at = |needle: &str, delta: usize| {
        json!({
            "textDocument": {"uri": people},
            "position": position(&text, needle, delta),
        })
    };
    let hover = session
        .request("textDocument/hover", at("(db/pull", 1))
        .unwrap();
    let definition = session
        .request("textDocument/definition", at("(db/pull", 1))
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- HOVER\n{}\n\n----- DEFINITION\n{}\n",
        hover["contents"]["value"].as_str().unwrap(),
        session.show_location(definition["uri"].as_str().unwrap(), &definition["range"])
    ));
    session.finish();
}

#[test]
fn no_diagnostics_for_declared_host_names() {
    let mut session = Session::start();
    let (people, _) = session.open_file("src/people.janet");
    assert_eq!(session.diagnostics(&people, 1), json!([]));
    session.finish();
}

#[test]
fn completion_offers_declared_names() {
    let mut session = Session::start();
    let (people, text) = session.open_file("src/people.janet");
    let items = session
        .request(
            "textDocument/completion",
            json!({
                "textDocument": {"uri": people},
                "position": position(&text, "(db/pull", 8),
            }),
        )
        .unwrap();
    let mut declared: Vec<_> = items
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| {
            item["label"]
                .as_str()
                .unwrap_or_default()
                .starts_with("db/")
        })
        .map(|item| {
            format!(
                "{} {}",
                item["label"].as_str().unwrap(),
                item["detail"].as_str().unwrap_or_default()
            )
        })
        .collect();
    declared.sort();
    insta::assert_snapshot!(format!("----- COMPLETIONS\n{}\n", declared.join("\n")));
    session.finish();
}

#[test]
fn an_unsaved_declaration_changes_hover_elsewhere() {
    let mut session = Session::start();
    let (people, text) = session.open_file("src/people.janet");
    let (host, declarations) = session.open_file("src/host.d.janet");
    let edited = declarations.replace(":ret Entity? :throws", ":ret Eid :throws");
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": host, "version": 2},
            "contentChanges": [{"text": edited}],
        }),
    );
    let hover = session
        .request(
            "textDocument/hover",
            json!({
                "textDocument": {"uri": people},
                "position": position(&text, "(db/pull", 1),
            }),
        )
        .unwrap();
    let hover = hover["contents"]["value"].as_str().unwrap();
    assert!(hover.contains("-> Eid"), "{hover}");
    session.finish();
}

#[test]
fn hover_and_definition_of_an_inline_declaration() {
    let mut session = Session::start();
    let source = concat!(
        "(comment :declare\n",
        "  (defn host/now {:params [] :ret :number} \"Seconds since the epoch.\" []))\n",
        "(host/now)\n",
    );
    let scratch = uri(&session.root.join("src/scratch.janet"));
    session.open(&scratch, source);
    let at = |needle: &str, delta: usize| {
        json!({
            "textDocument": {"uri": scratch},
            "position": position(source, needle, delta),
        })
    };
    let hover = session
        .request("textDocument/hover", at("(host/now)", 1))
        .unwrap();
    let definition = session
        .request("textDocument/definition", at("(host/now)", 1))
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- HOVER\n{}\n\n----- DEFINITION\n{}\n",
        hover["contents"]["value"].as_str().unwrap(),
        session.show_location(definition["uri"].as_str().unwrap(), &definition["range"])
    ));
    session.finish();
}

#[test]
fn deleting_the_declaration_file_brings_the_unknown_symbols_back() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/project");
    let root = std::env::temp_dir().join(format!("janet-zed-declarations-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(root.join("src")).unwrap();
    for name in ["src/people.janet", "src/host.d.janet"] {
        std::fs::copy(fixtures.join(name), root.join(name)).unwrap();
    }
    let root = root.canonicalize().unwrap();
    let mut session = Session::start_at(root.clone(), "src/people.janet");
    let people = session.report.clone();
    let text = session.text.clone();
    assert_eq!(session.diagnostics(&people, 1), json!([]));

    let host = root.join("src/host.d.janet");
    std::fs::remove_file(&host).unwrap();
    session.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": uri(&host), "type": 3}]}),
    );
    // The next edit is what checks the buffer again.
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": people, "version": 2},
            "contentChanges": [{"text": text}],
        }),
    );
    let mut unknown: Vec<String> = session
        .diagnostics(&people, 2)
        .as_array()
        .unwrap()
        .iter()
        .map(|problem| problem["message"].as_str().unwrap().to_string())
        .collect();
    unknown.sort();
    unknown.dedup();
    session.finish();
    std::fs::remove_dir_all(&root).ok();
    insta::assert_snapshot!(format!("----- PROBLEMS\n{}\n", unknown.join("\n")));
}

/// A core binding's hover reads its declared types, not only the call its docstring opens with.
#[test]
fn hover_on_a_core_binding() {
    let mut session = Session::start();
    let source = "(function? print)\n";
    let scratch = uri(&session.root.join("scratch.janet"));
    session.open(&scratch, source);
    let params = json!({
        "textDocument": {"uri": scratch},
        "position": position(source, "function?", 0),
    });
    let hover = session.request("textDocument/hover", params).unwrap();
    let shown = hover["contents"]["value"].as_str().unwrap().to_string();
    session.finish();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- HOVER\n{shown}\n"
    ));
}

/// A parameter is typed by the signature written for the function, and a binding by the value it
/// holds: a hover on either says so.
#[test]
fn hover_on_a_local_names_its_type() {
    let mut session = Session::start();
    let path = session.root.join("src/shapes.janet");
    let text = std::fs::read_to_string(&path).unwrap();
    let shapes = uri(&path);
    session.open(&shapes, &text);
    let mut hover = |needle: &str, delta: usize| {
        let params = json!({
            "textDocument": {"uri": shapes},
            "position": position(&text, needle, delta),
        });
        let hover = session.request("textDocument/hover", params).unwrap();
        hover["contents"]["value"].as_str().unwrap().to_string()
    };
    let shown = [("(shape :kind)", 1), (":r r}", 3), (":w w :h h}", 3)]
        .map(|(needle, delta)| format!("-- {needle}\n{}", hover(needle, delta)));
    insta::assert_snapshot!(format!("----- HOVER\n{}\n", shown.join("\n\n")));
    session.finish();
}

/// The type a local takes from an imported module's signature: `circle` is declared to answer a
/// `Circle`, so the name bound to the call is one.
#[test]
fn hover_on_a_local_typed_by_an_imported_signature() {
    let mut session = Session::start();
    let source =
        "(import ./shapes)\n\n(defn draw []\n  (let [c (shapes/circle 1)]\n    (shapes/area c)))\n";
    let scratch = uri(&session.root.join("src/scratch.janet"));
    session.open(&scratch, source);
    let params = json!({
        "textDocument": {"uri": scratch},
        "position": position(source, "area c", 5),
    });
    let hover = session.request("textDocument/hover", params).unwrap();
    let shown = hover["contents"]["value"].as_str().unwrap().to_string();
    session.finish();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- HOVER\n{shown}\n"
    ));
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

#[test]
fn hover_and_definition_from_a_running_repl() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let port = repl_port(1);
    let mut repl = runtime.block_on(Netrepl::connect("janet", port)).unwrap();
    let mut session = Session::start_with_repl(port);
    // Sent as the REPL kernel sends code it finds in a file: from line 2 of `src/shapes.janet`.
    let found_at = Position {
        path: session.root.join("src/shapes.janet"),
        line: 2,
        column: 1,
    };
    let code = "(defn from-repl \"Only the REPL knows.\" [] 1)";
    let evaluated = runtime.block_on(repl.eval(code, Some(&found_at))).unwrap();
    assert_eq!(evaluated.errors, "");

    let source = "(from-repl)\n";
    let scratch = uri(&session.root.join("scratch.janet"));
    session.open(&scratch, source);
    let params = json!({
        "textDocument": {"uri": scratch},
        "position": position(source, "from-repl", 0),
    });
    let hover = session
        .request("textDocument/hover", params.clone())
        .unwrap();
    let location = session.request("textDocument/definition", params).unwrap();
    insta::assert_snapshot!(format!(
        "----- REPL\n{code}\n\n----- SOURCE CODE\n{source}\n----- HOVER\n{}\n\n----- DEFINITION\n{}\n",
        hover["contents"]["value"].as_str().unwrap(),
        session.show_location(location["uri"].as_str().unwrap(), &location["range"])
    ));
    session.finish();
}

#[test]
fn hover_types_a_running_repl_declares() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let port = repl_port(2);
    let mut repl = runtime.block_on(Netrepl::connect("janet", port)).unwrap();
    let mut session = Session::start_with_repl(port);
    // The REPL knows both names; only one of them is written down in the buffer.
    let code = concat!(
        "(defn typed-in-repl {:params [:number] :ret :string} \"Only the REPL knows.\"\n",
        "  [id] (string id))\n",
        "(defn typed-in-source {:params [:number] :ret :boolean} [x] x)",
    );
    let evaluated = runtime.block_on(repl.eval(code, None)).unwrap();
    assert_eq!(evaluated.errors, "");

    let source = concat!(
        "(defn typed-in-source {:params [:string] :ret :nil} \"The source knows.\" [x] nil)\n",
        "(typed-in-repl 1)\n",
    );
    let scratch = uri(&session.root.join("scratch.janet"));
    session.open(&scratch, source);
    let hover = |session: &mut Session, needle: &str, delta: usize| {
        session
            .request(
                "textDocument/hover",
                json!({
                    "textDocument": {"uri": scratch},
                    "position": position(source, needle, delta),
                }),
            )
            .unwrap()["contents"]["value"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let only_in_repl = hover(&mut session, "(typed-in-repl 1)", 1);
    let in_both = hover(&mut session, "(defn typed-in-source", 6);
    insta::assert_snapshot!(format!(
        "----- REPL\n{code}\n\n----- SOURCE CODE\n{source}\n----- HOVER ON `typed-in-repl`\n\
         {only_in_repl}\n\n----- HOVER ON `typed-in-source`\n{in_both}\n"
    ));
    session.finish();
}

#[test]
fn completion_offers_the_keys_of_the_form_under_the_cursor() {
    let mut session = Session::start();
    let (people, text) = session.open_file("src/people.janet");
    let keys = |items: &Value| {
        items
            .as_array()
            .unwrap()
            .iter()
            .take_while(|item| item["kind"] == 5)
            .map(|item| {
                format!(
                    "{} {}",
                    item["label"].as_str().unwrap(),
                    item["detail"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
    };
    let at = |needle: &str, delta: usize| {
        json!({
            "textDocument": {"uri": people},
            "position": position(&text, needle, delta),
        })
    };
    let read = session
        .request("textDocument/completion", at("(request :body)", 9))
        .unwrap();
    let path = session
        .request("textDocument/completion", at("[:params :id]", 9))
        .unwrap();
    let destructured = session
        .request("textDocument/completion", at("{:tempids ids}", 1))
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- (request |:body)\n{}\n\n----- [:params |:id]\n{}\n\n----- (def {{|:tempids ids}}\n{}\n",
        keys(&read).join("\n"),
        keys(&path).join("\n"),
        keys(&destructured).join("\n"),
    ));
    session.finish();
}

#[test]
fn signature_help_instantiates_the_parameters_of_a_core_call() {
    let mut session = Session::start();
    let text = format!("{}\n(def mapped (map  [1 2 3]))\n", session.text);
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": session.report, "version": 2},
            "contentChanges": [{"text": text}],
        }),
    );
    let help = session
        .request(
            "textDocument/signatureHelp",
            json!({
                "textDocument": {"uri": session.report},
                "position": position(&text, "(map  [1 2 3])", 5),
            }),
        )
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- CALL\n(map | [1 2 3])\n\n----- SIGNATURE\n{}\n",
        help["signatures"][0]["label"].as_str().unwrap()
    ));
    session.finish();
}

#[test]
fn signature_help_inside_a_lambda_types_its_parameters() {
    let mut session = Session::start();
    let text = format!("{}\n(def mapped (map (fn [x] ) [1 2 3]))\n", session.text);
    session.notify(
        "textDocument/didChange",
        json!({
            "textDocument": {"uri": session.report, "version": 2},
            "contentChanges": [{"text": text}],
        }),
    );
    let help = session
        .request(
            "textDocument/signatureHelp",
            json!({
                "textDocument": {"uri": session.report},
                "position": position(&text, "(fn [x] )", 8),
            }),
        )
        .unwrap();
    insta::assert_snapshot!(format!(
        "----- CALL\n(map (fn [x] |) [1 2 3])\n\n----- SIGNATURE\n{}\n",
        help["signatures"][0]["label"].as_str().unwrap()
    ));
    session.finish();
}

/// The type diagnostics fixture, with a call the checker itself objects to appended.
fn with_a_broken_call(root: &Path) -> String {
    let path = root.join("../diagnostics/types.janet");
    let text = std::fs::read_to_string(&path).unwrap();
    format!("{text}\n(defn broken []\n  (no-such-function 1))\n")
}

/// `source` published for `uri` at version 1, as `source severity line: message`.
fn published(session: &mut Session, uri: &str) -> String {
    session
        .diagnostics(uri, 1)
        .as_array()
        .unwrap()
        .iter()
        .map(|problem| {
            format!(
                "{} {} {}: {}",
                problem["source"].as_str().unwrap(),
                problem["severity"],
                problem["range"]["start"]["line"],
                problem["message"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn types_report_nothing_until_they_are_asked_to() {
    let mut session = Session::start();
    let source = with_a_broken_call(&session.root);
    let scratch = uri(&session.root.join("src/mistakes.janet"));
    session.open(&scratch, &source);
    insta::assert_snapshot!(published(&mut session, &scratch));
    session.finish();
}

/// The Zed extension sends `"types": null` whenever nothing is configured under it; the server
/// serves the defaults rather than failing to start.
#[test]
fn a_null_types_block_leaves_the_defaults_standing() {
    let mut session = Session::start_with(
        Session::root(),
        "src/report.janet",
        &json!({"types": Value::Null}),
    );
    let hover = session
        .request("textDocument/hover", session.at("(shapes/area s", 9))
        .unwrap();
    assert!(
        hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("area")
    );
    session.finish();
}

#[test]
fn types_report_as_hints_beside_what_the_checker_found() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/project")
        .canonicalize()
        .unwrap();
    let settings = json!({"types": {"diagnostics": "hint"}});
    let mut session = Session::start_with(root, "src/report.janet", &settings);
    let source = with_a_broken_call(&session.root);
    let scratch = uri(&session.root.join("src/mistakes.janet"));
    session.open(&scratch, &source);
    insta::assert_snapshot!(published(&mut session, &scratch));
    session.finish();
}

#[test]
fn changing_the_setting_turns_the_types_on() {
    let mut session = Session::start();
    let source = with_a_broken_call(&session.root);
    let scratch = uri(&session.root.join("src/mistakes.janet"));
    session.open(&scratch, &source);
    let off = published(&mut session, &scratch);
    session.notify(
        "workspace/didChangeConfiguration",
        json!({"settings": {"types": {"diagnostics": "warning"}}}),
    );
    let on = published(&mut session, &scratch);
    insta::assert_snapshot!(format!("----- OFF\n{off}\n\n----- WARNING\n{on}\n"));
    session.finish();
}

#[test]
fn go_to_definition_from_an_arity_diagnostic_reaches_the_declaration() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/project")
        .canonicalize()
        .unwrap();
    let settings = json!({"types": {"diagnostics": "warning"}});
    let mut session = Session::start_with(root, "src/report.janet", &settings);
    let source = with_a_broken_call(&session.root);
    let scratch = uri(&session.root.join("src/mistakes.janet"));
    session.open(&scratch, &source);
    let published = session.diagnostics(&scratch, 1);
    let arity = published
        .as_array()
        .unwrap()
        .iter()
        .find(|problem| {
            problem["message"]
                .as_str()
                .unwrap()
                .contains("arguments, given")
        })
        .expect("the arity diagnostic");
    let found = session
        .request(
            "textDocument/definition",
            json!({
                "textDocument": {"uri": scratch},
                "position": arity["range"]["start"],
            }),
        )
        .unwrap();
    let declaration = session.show_location(found["uri"].as_str().unwrap(), &found["range"]);
    insta::assert_snapshot!(format!(
        "----- DIAGNOSTIC\n{}\n\n----- DEFINITION\n{declaration}\n",
        arity["message"].as_str().unwrap()
    ));
    session.finish();
}

/// A library macro's expansion: `:lint-as` gives the call the name it defines, and only the
/// checker, which compiled what the macro expanded to, has the types it declared there.
#[test]
fn a_library_macro_types_the_names_it_binds() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/exports");
    let root = std::env::temp_dir().join(format!("janet-zed-lint-as-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    // The library as `jpm -l` installs it: the module, and the config and declarations it exports.
    let installed = root.join("jpm_tree/lib");
    std::fs::create_dir_all(installed.join("janet-zed.exports/lib")).unwrap();
    for name in [
        "lib.janet",
        "janet-zed.exports/lib/config.jdn",
        "janet-zed.exports/lib/lib.d.janet",
    ] {
        std::fs::copy(fixtures.join(name), installed.join(name)).unwrap();
    }
    let source = concat!(
        "(import lib)\n\n",
        "(lib/defthing wheel \"Wheel\")\n\n",
        "(lib/shared shout (string/ascii-upper text))\n",
    );
    std::fs::write(root.join("main.janet"), source).unwrap();
    let root = root.canonicalize().unwrap();

    let mut session = Session::start_at(root.clone(), "main.janet");
    let main = session.report.clone();
    // The check is what expands the macros; its diagnostics say it has run.
    assert_eq!(session.diagnostics(&main, 1), json!([]));
    let mut hover = |needle: &str| {
        let at = json!({
            "textDocument": {"uri": main},
            "position": position(source, needle, 0),
        });
        session.request("textDocument/hover", at).unwrap()["contents"]["value"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let shown = format!("{}\n{}", hover("wheel \"Wheel\""), hover("shout ("));
    session.finish();
    std::fs::remove_dir_all(&root).ok();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- HOVER\n{shown}\n"
    ));
}

/// An installed spork the import resolves to: its own source declares no types, so the
/// declarations the server carries are still what type the names it exports.
#[test]
fn installed_spork_is_typed_by_the_declarations() {
    let root =
        std::env::temp_dir().join(format!("janet-zed-spork-installed-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let installed = root.join("jpm_tree/lib/spork");
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::write(
        installed.join("json.janet"),
        "(defn decode\n  \"Parse JSON.\"\n  [json-source &opt keywords nils]\n  @{})\n",
    )
    .unwrap();
    let source = "(import spork/json)\n\n(defn parse [text]\n  (json/decode text))\n";
    std::fs::write(root.join("main.janet"), source).unwrap();
    let root = root.canonicalize().unwrap();

    let mut session = Session::start_at(root.clone(), "main.janet");
    let main = session.report.clone();
    let params = json!({
        "textDocument": {"uri": main},
        "position": position(source, "json/decode", 5),
    });
    let hover = session.request("textDocument/hover", params).unwrap();
    let shown = hover["contents"]["value"].as_str().unwrap().to_string();
    session.finish();
    std::fs::remove_dir_all(&root).ok();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- HOVER\n{shown}\n"
    ));
}

/// The declarations the server carries for spork: a file that imports one of its modules is typed
/// by them, whatever the installed spork ships.
#[test]
fn spork_types_what_its_modules_answer() {
    let root = std::env::temp_dir().join(format!("janet-zed-spork-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let source = "(import spork/json)\n\n(defn parse [text]\n  (json/decode text))\n";
    std::fs::write(root.join("main.janet"), source).unwrap();
    let root = root.canonicalize().unwrap();

    let mut session = Session::start_at(root.clone(), "main.janet");
    let main = session.report.clone();
    let mut hover = |needle: &str| {
        let at = json!({
            "textDocument": {"uri": main},
            "position": position(source, needle, 0),
        });
        session.request("textDocument/hover", at).unwrap()["contents"]["value"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let shown = format!("{}\n{}", hover("json/decode"), hover("parse [text]"));
    session.finish();
    std::fs::remove_dir_all(&root).ok();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- HOVER\n{shown}\n"
    ));
}
