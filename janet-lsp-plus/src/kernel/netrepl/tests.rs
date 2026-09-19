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

/// SIGINT cancels the kernel's evaluation, spinning or waiting, and the REPL goes on.
#[cfg(unix)]
#[test]
fn an_interrupt_cancels_the_evaluation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let project = Path::new("/work/app");
    let mut repl = runtime
        .block_on(Netrepl::start(
            "janet",
            free_port().unwrap(),
            project,
            "secret",
        ))
        .unwrap();
    let pid = runtime.block_on(repl.pid()).unwrap();
    for code in ["(while true)", "(ev/sleep 100)"] {
        let interrupter = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            signal(pid, "INT").unwrap();
        });
        let evaluation = runtime.block_on(repl.eval(code, None)).unwrap();
        interrupter.join().unwrap();
        assert!(evaluation.errors.contains("interrupted"), "{evaluation:?}");
    }
    let after = runtime.block_on(repl.eval("(+ 1 2)", None)).unwrap();
    assert_eq!(after.value, "3");
}

/// Output comes as it is printed, before the evaluation ends, and `getline` asks the kernel.
#[test]
fn output_streams_and_getline_asks() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let messages = runtime.block_on(async {
        let mut repl = Netrepl::start("janet", free_port().unwrap(), Path::new("/work/app"), "t")
            .await
            .unwrap();
        repl.begin_eval(
            r#"(print "a") (ev/sleep 0.2) (print "b") (getline "name? ")"#,
            None,
        )
        .await
        .unwrap();
        let mut messages = Vec::new();
        loop {
            let message = repl.next().await.unwrap();
            if matches!(message, Message::Input(_)) {
                repl.answer("zed\n").await.unwrap();
            }
            let done = matches!(message, Message::Done(_));
            messages.push(message);
            if done {
                break messages;
            }
        }
    });
    assert_eq!(
        messages,
        [
            Message::Output("a\n".to_string()),
            Message::Output("b\n".to_string()),
            Message::Input("name? ".to_string()),
            Message::Done(Evaluation {
                value: r#"@"zed\n""#.to_string(),
                ..Evaluation::default()
            }),
        ]
    );
}
