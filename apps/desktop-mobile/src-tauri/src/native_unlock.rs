//! Asking for the master password on macOS with the system's own dialog (`osascript`'s `display dialog` with
//! a hidden answer), so unlocking starts no web page: no `WebKit` processes and no `AutoFill` helper.
//!
//! The password never appears in a command line or the environment: the script, which holds only the
//! prompt's words, goes to `osascript` on its standard input, and the answer comes back on its standard output,
//! read into memory that is wiped when dropped. Only one dialog is shown at a time; quitting the app closes it.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

use crate::commands::{AppCore, unlock_with};
use crate::core::PanelStatus;

/// The program that shows the dialog.
const OSASCRIPT: &str = "/usr/bin/osascript";
/// What `osascript` says on standard error when the person chose Cancel.
const CANCELLED: &str = "-128";
/// How many times a wrong password is asked for again before the dialog gives up.
const ATTEMPTS: usize = 5;

/// The dialog now shown, if any.
#[derive(Default)]
pub struct Prompt {
    asking: AtomicBool,
    child: Mutex<Option<Child>>,
}

/// The dialog's words in one language.
struct Words {
    title: &'static str,
    ask: &'static str,
    wrong: &'static str,
    empty: &'static str,
    cancel: &'static str,
    unlock: &'static str,
}

const EN: Words = Words {
    title: "OTPKeep",
    ask: "Enter the master password to open your data.",
    wrong: "The master password is not correct. Try again.",
    empty: "Enter the master password.",
    cancel: "Cancel",
    unlock: "Unlock",
};

const ZH: Words = Words {
    title: "OTPKeep",
    ask: "输入主口令以打开数据。",
    wrong: "主口令不正确，请重试。",
    empty: "请输入主口令。",
    cancel: "取消",
    unlock: "解锁",
};

/// Why the dialog is shown again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Again {
    Wrong,
    Empty,
}

/// What came of showing the dialog.
#[derive(Debug, PartialEq, Eq)]
enum Reply {
    Password(Zeroizing<String>),
    Cancelled,
    /// `osascript` could not run or failed in another way.
    Failed,
}

/// Asks for the master password with the system's dialog, off the main thread, then opens the data. When the
/// dialog cannot be shown at all, the app's own password window is opened instead.
pub fn start(app: &AppHandle) {
    let prompt = app.state::<Prompt>();
    if prompt.asking.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let fell_back = ask_until_open(&app);
        app.state::<Prompt>().asking.store(false, Ordering::SeqCst);
        if fell_back {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || crate::unlock::show_window(&handle));
        }
    });
}

/// Shows the dialog until the data opens, the person cancels or gives up; true when it could not be shown.
fn ask_until_open(app: &AppHandle) -> bool {
    let words = if crate::native::chinese(app) {
        &ZH
    } else {
        &EN
    };
    let mut again = None;
    for _ in 0..ATTEMPTS {
        let password = match ask(app, &script(words, again)) {
            Reply::Password(password) => password,
            Reply::Cancelled => return false,
            Reply::Failed => return true,
        };
        if password.is_empty() {
            again = Some(Again::Empty);
            continue;
        }
        let Some(core) = app.try_state::<AppCore>() else {
            return false;
        };
        let opened = tauri::async_runtime::block_on(unlock_with(&core, password));
        match opened {
            Ok(state) if state.status == PanelStatus::NeedsPassword => again = Some(Again::Wrong),
            Ok(_) | Err(_) => {
                let _ = app.emit("vault-changed", ());
                return false;
            }
        }
    }
    false
}

/// Runs one dialog; the child is kept so that quitting the app can close it.
fn ask(app: &AppHandle, script: &str) -> Reply {
    let spawned = Command::new(OSASCRIPT)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = spawned else {
        tracing::warn!("the system password dialog could not be started");
        return Reply::Failed;
    };
    let (Some(mut input), Some(mut output), Some(mut errors)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        return Reply::Failed;
    };
    let prompt = app.state::<Prompt>();
    *prompt.child.lock().unwrap_or_else(PoisonError::into_inner) = Some(child);
    let written = input.write_all(script.as_bytes());
    drop(input);
    let mut answer = Zeroizing::new(Vec::new());
    let read = output.read_to_end(&mut answer);
    let mut said = String::new();
    let _ = errors.read_to_string(&mut said);
    let child = prompt
        .child
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    let success = child
        .and_then(|mut child| child.wait().ok())
        .is_some_and(|status| status.success());
    if written.is_err() || read.is_err() {
        return Reply::Failed;
    }
    reply(success, answer, &said)
}

/// Closes a dialog still shown, when the app quits.
pub fn stop(app: &AppHandle) {
    if let Some(prompt) = app.try_state::<Prompt>()
        && let Some(child) = prompt
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
    {
        let _ = child.kill();
    }
}

/// The script: the dialog in the person's language, with why it is asked again; its answer is the script's
/// result, which `osascript` writes to standard output.
fn script(words: &Words, again: Option<Again>) -> String {
    let text = match again {
        None => words.ask.to_owned(),
        Some(Again::Wrong) => format!("{}\n\n{}", words.wrong, words.ask),
        Some(Again::Empty) => format!("{}\n\n{}", words.empty, words.ask),
    };
    let (cancel, unlock) = (quote(words.cancel), quote(words.unlock));
    format!(
        "activate\nset reply to display dialog {} default answer \"\" with hidden answer with title {} \
         with icon note buttons {{{cancel}, {unlock}}} default button {unlock} cancel button {cancel}\n\
         return text returned of reply\n",
        quote(&text),
        quote(words.title),
    )
}

/// `text` as an `AppleScript` string literal.
fn quote(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

/// What `osascript`'s exit, standard output and standard error mean.
fn reply(success: bool, output: Zeroizing<Vec<u8>>, errors: &str) -> Reply {
    if !success {
        return if errors.contains(CANCELLED) {
            Reply::Cancelled
        } else {
            Reply::Failed
        };
    }
    let mut bytes = output;
    // osascript ends its result with one line break.
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    match String::from_utf8(std::mem::take(&mut *bytes)) {
        Ok(text) => Reply::Password(Zeroizing::new(text)),
        Err(error) => {
            drop(Zeroizing::new(error.into_bytes()));
            Reply::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::{Again, EN, Reply, ZH, quote, reply, script};

    #[test]
    fn quotes_text_for_applescript() {
        assert_eq!(quote(r#"say "hi" \ bye"#), r#""say \"hi\" \\ bye""#);
        assert_eq!(quote("one\ntwo"), r#""one\ntwo""#);
    }

    #[test]
    fn the_dialog_hides_the_answer_and_names_its_buttons() {
        let first = script(&EN, None);

        assert!(first.contains("with hidden answer"));
        assert!(first.contains(r#"display dialog "Enter the master password to open your data.""#));
        assert!(first.contains(
            r#"buttons {"Cancel", "Unlock"} default button "Unlock" cancel button "Cancel""#
        ));
        assert!(first.ends_with("return text returned of reply\n"));
    }

    #[test]
    fn says_why_it_asks_again_in_both_languages() {
        assert!(
            script(&EN, Some(Again::Wrong))
                .contains(r#""The master password is not correct. Try again.\n\n"#)
        );
        assert!(script(&ZH, Some(Again::Wrong)).contains("主口令不正确，请重试。"));
        assert!(script(&ZH, Some(Again::Empty)).contains(r#"buttons {"取消", "解锁"}"#));
        assert!(script(&EN, Some(Again::Empty)).contains("Enter the master password.\\n\\n"));
    }

    #[test]
    fn reads_the_answer_without_the_line_break() {
        let answer = reply(true, Zeroizing::new(b"demo password\n".to_vec()), "");

        assert_eq!(
            answer,
            Reply::Password(Zeroizing::new("demo password".to_owned()))
        );
        assert_eq!(
            reply(true, Zeroizing::new(b"\n".to_vec()), ""),
            Reply::Password(Zeroizing::new(String::new()))
        );
    }

    #[test]
    fn cancel_and_failures_are_told_apart() {
        let none = || Zeroizing::new(Vec::new());

        assert_eq!(
            reply(false, none(), "execution error: User canceled. (-128)"),
            Reply::Cancelled
        );
        assert_eq!(reply(false, none(), "syntax error"), Reply::Failed);
        assert_eq!(
            reply(true, Zeroizing::new(vec![0xff, 0xfe]), ""),
            Reply::Failed
        );
    }
}
