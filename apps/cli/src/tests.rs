use super::{EXIT_OK, EXIT_USAGE, run};

fn run_with(args: &[&str]) -> (u8, String, String) {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

#[test]
fn version_prints_the_name_and_version_to_stdout() {
    let (code, out, err) = run_with(&["--version"]);

    assert_eq!(code, EXIT_OK);
    assert_eq!(out, format!("otpkeep {}\n", env!("CARGO_PKG_VERSION")));
    assert!(err.is_empty());
}

#[test]
fn version_subcommand_matches_the_flag() {
    assert_eq!(run_with(&["version"]), run_with(&["--version"]));
}

#[test]
fn help_lists_usage_on_stdout() {
    for flag in ["help", "--help", "-h"] {
        let (code, out, err) = run_with(&[flag]);

        assert_eq!(code, EXIT_OK, "{flag}");
        assert!(out.starts_with("Usage: otpkeep <command>"), "{flag}: {out}");
        assert!(out.contains("version"), "{flag}");
        assert!(err.is_empty(), "{flag}");
    }
}

#[test]
fn no_arguments_prints_help() {
    let (code, out, _) = run_with(&[]);

    assert_eq!(code, EXIT_OK);
    assert!(out.starts_with("Usage: otpkeep <command>"));
}

#[test]
fn an_unknown_command_is_a_usage_error_on_stderr() {
    let (code, out, err) = run_with(&["frobnicate"]);

    assert_eq!(code, EXIT_USAGE);
    assert!(out.is_empty());
    assert!(err.contains("unknown command 'frobnicate'"), "{err}");
    assert!(err.contains("otpkeep help"), "{err}");
}
