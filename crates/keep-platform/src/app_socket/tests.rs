use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;

use super::{
    AskError, MAX_LINE, MessageError, Request, Response, ask, listen, read_message, remove,
    socket_path, write_message,
};

/// A short private folder: socket paths are limited to about 100 bytes.
fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("sock-")
        .tempdir_in("/tmp")
        .unwrap()
}

#[test]
fn a_request_reads_back_as_written() {
    let mut line = Vec::new();
    write_message(&mut line, &Request::code("Example Cloud", true)).unwrap();

    assert_eq!(
        String::from_utf8(line.clone()).unwrap(),
        "{\"v\":1,\"op\":\"code\",\"query\":\"Example Cloud\",\"next\":true}\n"
    );
    let read: Request = read_message(line.as_slice()).unwrap();
    assert_eq!(read, Request::code("Example Cloud", true));
}

#[test]
fn next_may_be_left_out() {
    let read: Request =
        read_message(&b"{\"v\":1,\"op\":\"code\",\"query\":\"demo\"}\n"[..]).unwrap();

    assert!(!read.next);
}

#[test]
fn refuses_long_lines_other_versions_and_unknown_fields() {
    let long = format!(
        "{{\"v\":1,\"op\":\"code\",\"query\":\"{}\"}}\n",
        "x".repeat(MAX_LINE)
    );
    assert!(matches!(
        read_message::<Request>(long.as_bytes()),
        Err(MessageError::TooLong)
    ));
    assert!(matches!(
        read_message::<Request>(&b"{\"v\":2,\"op\":\"code\",\"query\":\"demo\"}\n"[..]),
        Err(MessageError::Invalid)
    ));
    assert!(matches!(
        read_message::<Request>(&b"{\"v\":1,\"op\":\"code\",\"query\":\"demo\",\"x\":1}\n"[..]),
        Err(MessageError::Invalid)
    ));
    assert!(matches!(
        read_message::<Request>(&b"not json\n"[..]),
        Err(MessageError::Invalid)
    ));
}

#[test]
fn answers_tell_codes_from_failures() {
    let code = Response::code("Example Cloud", "123456".to_owned(), Some(17));
    let locked = Response::error("LOCKED", "enter the master password in OTPKeep", Vec::new());
    let ambiguous = Response::error(
        "AMBIGUOUS",
        "'example' matches 2 accounts",
        vec!["Example Cloud".to_owned(), "Example Mail".to_owned()],
    );
    for answer in [code, locked, ambiguous] {
        let mut line = Vec::new();
        write_message(&mut line, &answer).unwrap();
        assert_eq!(read_message::<Response>(line.as_slice()).unwrap(), answer);
    }
}

#[test]
fn the_documented_examples_are_valid_messages() {
    let docs = [
        include_str!("../../../../docs/scripting.md"),
        include_str!("../../../../docs/scripting.zh-CN.md"),
    ];
    let mut seen = 0;
    for doc in docs {
        for line in doc
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("{\"v\":1"))
        {
            let line = format!("{line}\n");
            let request = read_message::<Request>(line.as_bytes());
            let response = read_message::<Response>(line.as_bytes());
            assert!(request.is_ok() || response.is_ok(), "{line}");
            seen += 1;
        }
    }
    assert!(seen >= 8, "{seen}");
}

#[test]
fn the_socket_and_its_folder_are_private() {
    let dir = temp_dir();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

    let _listener = listen(dir.path()).unwrap();

    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(dir.path()), 0o700);
    assert_eq!(mode(&socket_path(dir.path())), 0o600);
}

#[test]
fn a_socket_left_behind_is_replaced_but_a_live_one_is_kept() {
    let dir = temp_dir();
    drop(UnixListener::bind(socket_path(dir.path())).unwrap());

    let live = listen(dir.path()).unwrap();

    let again = listen(dir.path());
    assert_eq!(
        again.err().map(|e| e.kind()),
        Some(std::io::ErrorKind::AddrInUse)
    );
    drop(live);
    remove(dir.path());
    assert!(!socket_path(dir.path()).exists());
}

#[test]
fn asks_the_app_and_reads_its_answer() {
    let dir = temp_dir();
    let listener = listen(dir.path()).unwrap();
    let app = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let request: Request = read_message(&stream).unwrap();
        let answer = Response::code(&request.query, "123456".to_owned(), Some(20));
        write_message(&stream, &answer).unwrap();
    });

    let answer = ask(dir.path(), &Request::code("Example Cloud", false)).unwrap();

    app.join().unwrap();
    assert_eq!(
        answer,
        Response::code("Example Cloud", "123456".to_owned(), Some(20))
    );
}

#[test]
fn no_app_means_not_running() {
    let dir = temp_dir();
    drop(UnixListener::bind(socket_path(dir.path())).unwrap());

    assert!(matches!(
        ask(dir.path(), &Request::code("demo", false)),
        Err(AskError::NotRunning)
    ));
    assert!(matches!(
        ask(&dir.path().join("missing"), &Request::code("demo", false)),
        Err(AskError::NotRunning)
    ));
}

#[test]
fn reads_only_the_first_line() {
    let two =
        b"{\"v\":1,\"op\":\"code\",\"query\":\"a\"}\n{\"v\":1,\"op\":\"code\",\"query\":\"b\"}\n";
    let mut reader = BufReader::new(&two[..]);
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();

    let read: Request = read_message(&two[..]).unwrap();

    assert_eq!(read.query, "a");
    assert!(first.ends_with('\n'));
}
