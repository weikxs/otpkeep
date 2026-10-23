//! Entry point of the `otpkeep` binary.

use std::io::{stderr, stdout};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(otpkeep_cli::run(
        &args,
        &mut stdout().lock(),
        &mut stderr().lock(),
    ))
}
