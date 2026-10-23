//! Runs the built `otpkeep` binary the way a user would.

use std::process::Command;

fn otpkeep(args: &[&str]) -> std::process::Output {
    let result = Command::new(env!("CARGO_BIN_EXE_otpkeep"))
        .args(args)
        .output();
    match result {
        Ok(output) => output,
        Err(error) => unreachable!("the test binary could not start: {error}"),
    }
}

#[test]
fn version_exits_zero_and_prints_to_stdout() {
    let output = otpkeep(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("otpkeep {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn an_unknown_command_exits_with_the_usage_code() {
    let output = otpkeep(&["frobnicate"]);

    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown command"));
}
