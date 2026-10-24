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
    Lan {
        action: LanAction,
        options: Options,
    },
}

/// Which transports a pairing or sync may use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Transport {
    /// Both the local network and Bluetooth.
    #[default]
    Any,
    /// The local network only.
    Lan,
    /// Bluetooth only.
    Bluetooth,
}

/// Pairing, sync and device commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LanAction {
    SyncWait {
        minutes: Option<u64>,
        via: Transport,
    },
    SyncJoin {
        to: Option<String>,
        via: Transport,
    },
    PairOpen {
        minutes: Option<u64>,
        name: Option<String>,
        qr: bool,
        via: Transport,
    },
    PairJoin {
        invite: Option<String>,
        to: Option<String>,
        name: Option<String>,
        replace: bool,
        via: Transport,
    },
    Devices,
    RemoveDevice {
        fingerprint: String,
    },
    Leave,
    Rename {
        name: String,
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
        "sync" | "pair" | "devices" | "group" | "rename" => parse_lan(command, rest),
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

/// Flags of the LAN commands: words, flags with values, and plain switches.
struct LanArgs {
    words: Vec<String>,
    values: Vec<(String, String)>,
    switches: Vec<String>,
    options: Options,
}

impl LanArgs {
    fn value(&self, name: &str) -> Option<String> {
        self.values
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    fn switch(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }
}

fn parse_lan_flags(
    rest: &[String],
    valued: &[&str],
    switches: &[&str],
) -> Result<LanArgs, UsageError> {
    let mut parsed = LanArgs {
        words: Vec::new(),
        values: Vec::new(),
        switches: Vec::new(),
        options: Options::default(),
    };
    let mut args = rest.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            flag if valued.contains(&flag) => {
                let value = args
                    .next()
                    .ok_or_else(|| UsageError(format!("'{flag}' needs a value")))?;
                parsed.values.push((flag.to_owned(), value.clone()));
            }
            flag if switches.contains(&flag) => parsed.switches.push(flag.to_owned()),
            flag if flag.starts_with("--") => apply(&mut parsed.options, flag)?,
            word => parsed.words.push(word.to_owned()),
        }
    }
    Ok(parsed)
}

fn minutes(parsed: &LanArgs) -> Result<Option<u64>, UsageError> {
    parsed
        .value("--minutes")
        .map(|m| match m.parse::<u64>() {
            Ok(n) if (1..=5).contains(&n) => Ok(n),
            _ => Err(UsageError("'--minutes' must be 1 to 5".to_owned())),
        })
        .transpose()
}

fn parse_lan(command: &str, rest: &[String]) -> Result<Command, UsageError> {
    let (sub, rest) = match command {
        "devices" | "rename" => ("", rest),
        _ => match rest.split_first() {
            Some((sub, rest)) => (sub.as_str(), rest),
            None => {
                return Err(UsageError(format!(
                    "'{command}' needs a subcommand; see 'otpkeep help'"
                )));
            }
        },
    };
    let parsed = parse_lan_flags(
        rest,
        &["--minutes", "--to", "--name", "--invite", "--via"],
        &["--qr", "--replace"],
    )?;
    let via = match parsed.value("--via").as_deref() {
        None => Transport::Any,
        Some("lan") => Transport::Lan,
        Some("bluetooth") => Transport::Bluetooth,
        Some(other) => {
            return Err(UsageError(format!(
                "unknown '--via {other}'; use lan or bluetooth"
            )));
        }
    };
    let action = match (command, sub) {
        ("sync", "wait") => LanAction::SyncWait {
            minutes: minutes(&parsed)?,
            via,
        },
        ("sync", "join") => LanAction::SyncJoin {
            to: parsed.value("--to"),
            via,
        },
        ("pair", "open") => LanAction::PairOpen {
            minutes: minutes(&parsed)?,
            name: parsed.value("--name"),
            qr: parsed.switch("--qr"),
            via,
        },
        ("pair", "join") => LanAction::PairJoin {
            invite: parsed.value("--invite"),
            to: parsed.value("--to"),
            name: parsed.value("--name"),
            replace: parsed.switch("--replace"),
            via,
        },
        ("devices", "") => match parsed.words.as_slice() {
            [] => LanAction::Devices,
            [verb, fingerprint] if verb == "remove" => LanAction::RemoveDevice {
                fingerprint: fingerprint.clone(),
            },
            _ => {
                return Err(UsageError(
                    "use 'otpkeep devices' or 'otpkeep devices remove <fingerprint>'".to_owned(),
                ));
            }
        },
        ("group", "leave") => LanAction::Leave,
        ("rename", "") => match parsed.words.as_slice() {
            [name] => LanAction::Rename { name: name.clone() },
            _ => {
                return Err(UsageError(
                    "'rename' needs one name, quoted if it has spaces".to_owned(),
                ));
            }
        },
        _ => return Err(UsageError(format!("unknown command '{command} {sub}'"))),
    };
    let takes_words = matches!(
        action,
        LanAction::RemoveDevice { .. } | LanAction::Rename { .. }
    );
    if !takes_words && let Some(word) = parsed.words.first() {
        return Err(UsageError(format!("unexpected argument '{word}'")));
    }
    Ok(Command::Lan {
        action,
        options: parsed.options,
    })
}

fn apply(options: &mut Options, flag: &str) -> Result<(), UsageError> {
    match flag {
        "--json" => options.json = true,
        "--password-stdin" => options.password_stdin = true,
        other => return Err(UsageError(format!("unknown option '{other}'"))),
    }
    Ok(())
}
