//! Command-line arguments.

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    Version,
    Code {
        query: Vec<String>,
        next: bool,
        options: Options,
    },
    List {
        tag: Option<String>,
        options: Options,
    },
}

/// Flags shared by the vault commands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Options {
    pub(crate) json: bool,
    pub(crate) password_stdin: bool,
}

/// Why the command line was not understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageError(pub(crate) String);

pub(crate) fn parse(args: &[String]) -> Result<Command, UsageError> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    match command.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "version" | "--version" | "-V" => Ok(Command::Version),
        "code" => parse_code(rest),
        "list" => parse_list(rest),
        other => Err(UsageError(format!("unknown command '{other}'"))),
    }
}

fn parse_code(rest: &[String]) -> Result<Command, UsageError> {
    let mut query = Vec::new();
    let mut next = false;
    let mut options = Options::default();
    for arg in rest {
        match arg.as_str() {
            "--next" => next = true,
            flag if flag.starts_with("--") => apply(&mut options, flag)?,
            word => query.push(word.to_owned()),
        }
    }
    if query.is_empty() {
        return Err(UsageError(
            "'code' needs a search, for example: otpkeep code github".to_owned(),
        ));
    }
    Ok(Command::Code {
        query,
        next,
        options,
    })
}

fn parse_list(rest: &[String]) -> Result<Command, UsageError> {
    let mut tag = None;
    let mut options = Options::default();
    let mut args = rest.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tag" => {
                let value = args
                    .next()
                    .ok_or_else(|| UsageError("'--tag' needs a tag".to_owned()))?;
                tag = Some(value.clone());
            }
            flag if flag.starts_with("--") => apply(&mut options, flag)?,
            other => return Err(UsageError(format!("unexpected argument '{other}'"))),
        }
    }
    Ok(Command::List { tag, options })
}

fn apply(options: &mut Options, flag: &str) -> Result<(), UsageError> {
    match flag {
        "--json" => options.json = true,
        "--password-stdin" => options.password_stdin = true,
        other => return Err(UsageError(format!("unknown option '{other}'"))),
    }
    Ok(())
}
