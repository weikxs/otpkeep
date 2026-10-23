//! Command-line arguments.

use std::path::PathBuf;

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
    Init {
        master_password: bool,
        options: Options,
    },
    Import {
        path: PathBuf,
        transfer: Transfer,
        options: Options,
    },
    Export {
        format: ExportFormat,
        out: PathBuf,
        transfer: Transfer,
        options: Options,
    },
}

/// What `export` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    /// An OTPKeep backup, protected by a backup password.
    Backup,
    /// A link sheet: one `otpauth://` link per account.
    Uris,
    /// A `.secrets` file for GoldenPassport.
    GoldenPassport,
    /// Google Authenticator transfer codes, as SVG images.
    Google,
}

impl ExportFormat {
    /// Whether the file holds the secrets unencrypted.
    pub(crate) fn is_plaintext(self) -> bool {
        self != Self::Backup
    }

    fn parse(name: &str) -> Result<Self, UsageError> {
        match name {
            "backup" => Ok(Self::Backup),
            "uris" => Ok(Self::Uris),
            "goldenpassport" => Ok(Self::GoldenPassport),
            "google" => Ok(Self::Google),
            other => Err(UsageError(format!(
                "unknown format '{other}'; use backup, uris, goldenpassport or google"
            ))),
        }
    }
}

/// Flags of `import` and `export`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Transfer {
    /// Confirmed on the command line.
    pub(crate) yes: bool,
    /// The user accepts an unencrypted file.
    pub(crate) plaintext: bool,
    /// Read the backup password from the first line of this file.
    pub(crate) backup_password_file: Option<PathBuf>,
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
        "init" => parse_init(rest),
        "import" => parse_import(rest),
        "export" => parse_export(rest),
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

fn parse_init(rest: &[String]) -> Result<Command, UsageError> {
    let mut master_password = false;
    let mut options = Options::default();
    for arg in rest {
        match arg.as_str() {
            "--master-password" => master_password = true,
            flag if flag.starts_with("--") => apply(&mut options, flag)?,
            other => return Err(UsageError(format!("unexpected argument '{other}'"))),
        }
    }
    Ok(Command::Init {
        master_password,
        options,
    })
}

fn parse_import(rest: &[String]) -> Result<Command, UsageError> {
    let parsed = parse_transfer(rest, &[])?;
    let [path] = parsed.words.as_slice() else {
        return Err(UsageError(
            "'import' needs one file, for example: otpkeep import GoldenPassport.secrets"
                .to_owned(),
        ));
    };
    Ok(Command::Import {
        path: PathBuf::from(path),
        transfer: parsed.transfer,
        options: parsed.options,
    })
}

fn parse_export(rest: &[String]) -> Result<Command, UsageError> {
    let parsed = parse_transfer(rest, &["--format", "--out"])?;
    if let Some(word) = parsed.words.first() {
        return Err(UsageError(format!("unexpected argument '{word}'")));
    }
    let value = |name: &str| {
        parsed
            .values
            .iter()
            .find(|(flag, _)| flag == name)
            .map(|(_, value)| value.clone())
            .ok_or_else(|| UsageError(format!("'export' needs {name}")))
    };
    let format = ExportFormat::parse(&value("--format")?)?;
    let out = PathBuf::from(value("--out")?);
    if format.is_plaintext() && !parsed.transfer.plaintext {
        return Err(UsageError(
            "this format holds every secret unencrypted; add --plaintext if that is what you want"
                .to_owned(),
        ));
    }
    Ok(Command::Export {
        format,
        out,
        transfer: parsed.transfer,
        options: parsed.options,
    })
}

/// What `import` and `export` were given.
struct TransferArgs {
    words: Vec<String>,
    values: Vec<(String, String)>,
    transfer: Transfer,
    options: Options,
}

/// Reads transfer flags; `valued` names the extra flags that take a value.
fn parse_transfer(rest: &[String], valued: &[&str]) -> Result<TransferArgs, UsageError> {
    let mut parsed = TransferArgs {
        words: Vec::new(),
        values: Vec::new(),
        transfer: Transfer::default(),
        options: Options::default(),
    };
    let mut args = rest.iter();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .cloned()
                .ok_or_else(|| UsageError(format!("'{arg}' needs a value")))
        };
        match arg.as_str() {
            "--yes" | "-y" => parsed.transfer.yes = true,
            "--plaintext" => parsed.transfer.plaintext = true,
            "--backup-password-file" => {
                parsed.transfer.backup_password_file = Some(PathBuf::from(value()?));
            }
            flag if valued.contains(&flag) => {
                let taken = value()?;
                parsed.values.push((flag.to_owned(), taken));
            }
            flag if flag.starts_with("--") => apply(&mut parsed.options, flag)?,
            word => parsed.words.push(word.to_owned()),
        }
    }
    Ok(parsed)
}

fn apply(options: &mut Options, flag: &str) -> Result<(), UsageError> {
    match flag {
        "--json" => options.json = true,
        "--password-stdin" => options.password_stdin = true,
        other => return Err(UsageError(format!("unknown option '{other}'"))),
    }
    Ok(())
}
