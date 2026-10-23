//! The `otpkeep` command line.

use std::io::Write;

/// The command finished successfully.
pub const EXIT_OK: u8 = 0;
/// The command line could not be understood.
pub const EXIT_USAGE: u8 = 64;

const NAME: &str = "otpkeep";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
Usage: otpkeep <command>

Commands:
  version    Print the version
  help       Print this help
";

/// Runs the command line with `args` (without the program name) and returns the exit code.
///
/// Results go to `out`, errors to `err`. Write failures are ignored: there is nowhere left to report them.
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> u8 {
    match args.first().map(String::as_str) {
        None | Some("help" | "--help" | "-h") => {
            let _ = out.write_all(HELP.as_bytes());
            EXIT_OK
        }
        Some("version" | "--version" | "-V") => {
            let _ = writeln!(out, "{NAME} {VERSION}");
            EXIT_OK
        }
        Some(other) => {
            let _ = writeln!(
                err,
                "{NAME}: unknown command '{other}'\nRun 'otpkeep help' for usage."
            );
            EXIT_USAGE
        }
    }
}

#[cfg(test)]
mod tests;
