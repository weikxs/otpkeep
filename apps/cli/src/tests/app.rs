//! `otpkeep code` while the app runs: asked through the app's socket, falling back to the vault.

use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread::JoinHandle;

use keep_platform::MemoryKeystore;
use keep_platform::app_socket::{self, Request, Response};
use keep_vault::store::{VaultLock, VaultPaths};

use super::{WAIT, cli, expected_code, seed, temp_dir};
use crate::{EXIT_MULTIPLE_MATCHES, EXIT_NO_MATCH, EXIT_OK, EXIT_SCRIPTS_DISABLED};

/// A stand-in for the app: answers one request with `answer` and returns what it was asked.
fn fake_app(dir: &Path, answer: Response) -> JoinHandle<Request> {
    let listener = app_socket::listen(dir).unwrap();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let request: Request = app_socket::read_message(&stream).unwrap();
        app_socket::write_message(&stream, &answer).unwrap();
        request
    })
}

#[test]
fn asks_the_running_app_instead_of_waiting_for_the_vault() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    // The app holds the vault.
    let _held = VaultLock::acquire(&VaultPaths::new(dir.path()), WAIT).unwrap();
    let app = fake_app(
        dir.path(),
        Response::code("Example Cloud", "123456".to_owned(), Some(17)),
    );

    let run = cli(
        Some(dir.path()),
        &keystore,
        &["code", "example", "--json"],
        "",
    );

    assert_eq!(run.code, EXIT_OK, "{}", run.err);
    assert_eq!(
        run.out,
        "{\"data\":{\"code\":\"123456\",\"title\":\"Example Cloud\",\"valid_for_s\":17},\"ok\":true}\n"
    );
    assert_eq!(app.join().unwrap(), Request::code("example", false));
}

#[test]
fn the_apps_refusals_keep_the_command_lines_exit_codes() {
    let cases = [
        (
            Response::error("NOT_FOUND", "nothing", Vec::new()),
            EXIT_NO_MATCH,
            "NO_MATCH",
        ),
        (
            Response::error(
                "AMBIGUOUS",
                "'demo' matches 2 accounts",
                vec!["Demo Git".to_owned(), "Demo Git Staging".to_owned()],
            ),
            EXIT_MULTIPLE_MATCHES,
            "MULTIPLE_MATCHES",
        ),
        (
            Response::error("DISABLED", "turn it on in Settings", Vec::new()),
            EXIT_SCRIPTS_DISABLED,
            "DISABLED",
        ),
    ];
    for (answer, exit, code) in cases {
        let dir = temp_dir();
        let keystore = MemoryKeystore::new();
        let app = fake_app(dir.path(), answer);

        let run = cli(Some(dir.path()), &keystore, &["code", "demo", "--json"], "");

        app.join().unwrap();
        assert_eq!(run.code, exit);
        assert!(
            run.out.contains(&format!("\"code\":\"{code}\"")),
            "{}",
            run.out
        );
    }
}

#[test]
fn a_locked_app_or_a_socket_left_behind_falls_back_to_the_vault() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    let app = fake_app(
        dir.path(),
        Response::error("LOCKED", "unlock it", Vec::new()),
    );

    let locked = cli(Some(dir.path()), &keystore, &["code", "example"], "");
    app.join().unwrap();
    drop(UnixListener::bind(app_socket::socket_path(dir.path())).ok());
    let left_behind = cli(Some(dir.path()), &keystore, &["code", "example"], "");

    assert_eq!(locked.code, EXIT_OK, "{}", locked.err);
    assert_eq!(locked.out, format!("{}\n", expected_code(1)));
    assert_eq!(left_behind.out, format!("{}\n", expected_code(1)));
}
