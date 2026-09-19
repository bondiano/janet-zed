use super::*;

// Real replies from netrepl, see eval.janet.
macro_rules! assert_reply {
    ($reply:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            format!(
                "----- REPLY\n{}\n\n----- EVALUATION\n{:#?}\n",
                $reply,
                parse_reply($reply)
            ),
            $reply
        )
    };
}

#[test]
fn value_and_output() {
    assert_reply!(r#"(true ("43" "hi\n" ""))"#);
}

#[test]
fn escaped_bytes() {
    assert_reply!(r#"(true ("\"\xD0\xB9\\\"" "\e[32m\0" "boom"))"#);
}

#[test]
fn failure() {
    assert_reply!(r#"(false "oops")"#);
}

/// A port is recorded at the project's own path under the port files, and read back from there.
#[test]
fn a_recorded_port_is_read_back_from_the_project_path() {
    let ports = std::env::temp_dir().join(format!("janet-zed-ports-{}", std::process::id()));
    let project = Path::new("/work/app");
    let file = port_file_under(&ports, project);
    write_port(&ports, &file, 41_234).unwrap();
    let read = read_port(&file);
    std::fs::remove_dir_all(&ports).unwrap();
    assert_eq!(file, ports.join("work/app/port"));
    assert_eq!(read, Some(41_234));
    assert_eq!(
        port_file_under(&ports, Path::new("C:\\work")),
        ports.join("C\\work/port")
    );
}

/// A kernel in `src/` shares the REPL of the project around it.
#[test]
fn a_directory_belongs_to_the_project_around_it() {
    let root = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
    let project = root.parent().unwrap();
    assert_eq!(project_of(&root.join("src/kernel")), project);
    assert_eq!(project_of(project), project);
}

/// A kernel never connects to a server someone else started on its port, and a server answers
/// with the project it serves.
#[test]
fn a_server_is_its_own_and_names_its_project() {
    let taken = std::net::TcpListener::bind((HOST, 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let project = Path::new("/work/app");
    assert!(
        runtime
            .block_on(Netrepl::start("janet", port, project))
            .is_err()
    );
    drop(taken);
    let reply = runtime.block_on(async {
        let mut repl = Netrepl::start("janet", free_port().unwrap(), project)
            .await
            .unwrap();
        repl.call(PROJECT).await.unwrap()
    });
    assert!(serves(&reply, project), "{reply}");
    assert!(!serves(&reply, Path::new("/work/other")));
}
