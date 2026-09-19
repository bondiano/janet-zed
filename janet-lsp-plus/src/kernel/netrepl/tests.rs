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

/// A port and its token are recorded at the project's own path under the port files, the token
/// readable by this user only, and read back from there.
#[test]
fn a_record_is_read_back_from_the_project_path() {
    let ports = std::env::temp_dir().join(format!("janet-zed-ports-{}", std::process::id()));
    let dir = record_dir_under(&ports, Path::new("/work/app"));
    write_record(&ports, &dir, 41_234, "secret").unwrap();
    let read = read_record(&dir);
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(dir.join("token"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    std::fs::remove_dir_all(&ports).unwrap();
    assert_eq!(dir, ports.join("work/app"));
    assert_eq!(read, Some((41_234, "secret".to_string())));
    #[cfg(unix)]
    assert_eq!(mode, 0o600);
    assert_eq!(
        record_dir_under(&ports, Path::new("C:\\work")),
        ports.join("C\\work")
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
/// with the project it serves, to its token's holders only.
#[test]
fn a_server_is_its_own_and_serves_its_token() {
    let taken = std::net::TcpListener::bind((HOST, 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let project = Path::new("/work/app");
    assert!(
        runtime
            .block_on(Netrepl::start("janet", port, project, "secret"))
            .is_err()
    );
    drop(taken);
    let port = free_port().unwrap();
    let address = format!("{HOST}:{port}");
    let (reply, second, stranger) = runtime.block_on(async {
        let mut repl = Netrepl::start("janet", port, project, "secret")
            .await
            .unwrap();
        let reply = repl.call(PROJECT).await.unwrap();
        // netrepl renames a second client of the same name, which keeps the token in front.
        let mut second = Netrepl::attach(&address, "secret").await.unwrap();
        let second = second.call("(+ 1 2)").await.unwrap();
        let stranger = match Netrepl::attach(&address, "zed").await {
            Ok(mut stranger) => stranger.call("(+ 1 2)").await,
            Err(err) => Err(err),
        };
        (reply, second, stranger)
    });
    assert!(serves(&reply, project), "{reply}");
    assert!(!serves(&reply, Path::new("/work/other")));
    assert_eq!(second, "(true 3)");
    assert!(stranger.is_err(), "{stranger:?}");
}
