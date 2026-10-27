//! The local socket through which scripts get codes from the running app (docs/scripting.md; the protocol is in
//! [`keep_platform::app_socket`]).
//!
//! One thread waits in `accept` and does nothing else: no timer, no polling. Each connection carries one
//! request, read with a size limit and a short timeout, answered on the same thread. Codes are never logged.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use keep_platform::app_socket::{self, MessageError, Request, Response};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::AppCore;
use crate::hotkeys::Hotkeys;

/// The listening thread's stack: it only parses a short line and asks the core.
const STACK_BYTES: usize = 256 * 1024;

/// Where the socket is, to remove it when the app quits.
pub struct ScriptSocket(PathBuf);

/// Starts listening in `data_dir`; logs and carries on without it when that is not possible.
pub fn start(app: &AppHandle, data_dir: &Path) {
    let listener = match app_socket::listen(data_dir) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!(%error, "scripts cannot get codes from the app: the socket could not be opened");
            return;
        }
    };
    app.manage(ScriptSocket(data_dir.to_owned()));
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("otpkeep-scripts".to_owned())
        .stack_size(STACK_BYTES)
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(&app, &stream);
            }
        });
    if spawned.is_err() {
        tracing::warn!("scripts cannot get codes from the app: no thread for the socket");
        app_socket::remove(data_dir);
    }
}

/// Removes the socket when the app quits.
pub fn stop(app: &AppHandle) {
    if let Some(socket) = app.try_state::<ScriptSocket>() {
        app_socket::remove(&socket.0);
    }
}

/// Answers one connection.
fn serve(app: &AppHandle, stream: &UnixStream) {
    let timeouts = stream
        .set_read_timeout(Some(app_socket::TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(app_socket::TIMEOUT)));
    if timeouts.is_err() {
        return;
    }
    let answer = match app_socket::read_message::<Request>(stream) {
        Ok(request) => answer(app, &request),
        Err(MessageError::TooLong | MessageError::Invalid) => Response::error(
            "BAD_REQUEST",
            "send one line: {\"v\":1,\"op\":\"code\",\"query\":\"…\"}",
            Vec::new(),
        ),
        Err(MessageError::Io(_)) => return,
    };
    let _ = app_socket::write_message(stream, &answer);
}

fn answer(app: &AppHandle, request: &Request) -> Response {
    if request.op != "code" {
        return Response::error("BAD_REQUEST", "the only operation is \"code\"", Vec::new());
    }
    let (Some(core), Some(hotkeys)) = (app.try_state::<AppCore>(), app.try_state::<Hotkeys>())
    else {
        return Response::error("FAILED", "OTPKeep is starting or quitting", Vec::new());
    };
    let master = core.get().uses_master_password();
    if !hotkeys.settings().scripts_allowed(master) {
        return Response::error(
            "DISABLED",
            "scripts may not get codes from the running app; turn on \"Allow scripts to get codes\" in OTPKeep's Settings",
            Vec::new(),
        );
    }
    let given = core.get().script_code(&request.query, request.next);
    if request.next && matches!(given, Response::Code(_)) {
        // The counter moved on: the windows and the menu show it.
        let _ = app.emit("vault-changed", ());
    }
    given
}
