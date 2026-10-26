use std::fs;

use super::{FILE_NAME, Log, LogFile};

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-log-")
        .tempdir()
        .unwrap()
}

#[test]
fn keeps_two_files_under_the_size_limit() {
    let dir = temp_dir();
    let path = dir.path().join(FILE_NAME);
    let mut file = LogFile::new(path.clone(), 100);

    for n in 0..20 {
        file.write_line(&format!("line {n:02} of the demo log\n"));
    }

    let mut names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["otpkeep.log", "otpkeep.log.1"]);
    for name in names {
        assert!(fs::metadata(dir.path().join(name)).unwrap().len() <= 100);
    }
    assert!(fs::read_to_string(&path).unwrap().contains("line 19"));
}

#[test]
fn writes_warnings_with_their_fields_and_leaves_out_the_rest() {
    let dir = temp_dir();
    let log = Log(std::sync::Arc::default());

    tracing::subscriber::with_default(log.clone(), || {
        tracing::warn!(kind = "demo", "before the folder is known");
        tracing::info!("not written");
    });
    log.attach(dir.path()).unwrap();
    tracing::subscriber::with_default(log, || tracing::error!("after"));

    let text = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
    assert!(text.contains("WARN"), "{text}");
    assert!(
        text.contains("before the folder is known kind=\"demo\""),
        "{text}"
    );
    assert!(text.contains("ERROR"), "{text}");
    assert!(!text.contains("not written"), "{text}");
}

#[cfg(unix)]
#[test]
fn only_its_owner_can_read_it() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir();
    let path = dir.path().join(FILE_NAME);

    LogFile::new(path.clone(), 100).write_line("demo\n");

    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
