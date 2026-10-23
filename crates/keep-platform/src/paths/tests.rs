use std::path::PathBuf;

use super::{Os, data_dir_for};

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_owned())
    }
}

#[test]
fn macos_uses_application_support() {
    let dir = data_dir_for(Os::MacOs, env(&[("HOME", "/tmp/home")]));

    assert_eq!(
        dir,
        Some(PathBuf::from(
            "/tmp/home/Library/Application Support/OTPKeep"
        ))
    );
}

#[test]
fn windows_uses_local_not_roaming_app_data() {
    let dir = data_dir_for(
        Os::Windows,
        env(&[
            ("LOCALAPPDATA", r"C:\Demo\Local"),
            ("APPDATA", r"C:\Demo\Roaming"),
        ]),
    );

    assert_eq!(dir, Some(PathBuf::from(r"C:\Demo\Local").join("OTPKeep")));
}

#[test]
fn linux_follows_xdg_and_falls_back_to_local_share() {
    assert_eq!(
        data_dir_for(
            Os::Linux,
            env(&[("XDG_DATA_HOME", "/tmp/xdg"), ("HOME", "/tmp/home")])
        ),
        Some(PathBuf::from("/tmp/xdg/otpkeep"))
    );
    assert_eq!(
        data_dir_for(Os::Linux, env(&[("HOME", "/tmp/home")])),
        Some(PathBuf::from("/tmp/home/.local/share/otpkeep"))
    );
}

#[test]
fn a_relative_or_empty_xdg_value_is_ignored() {
    assert_eq!(
        data_dir_for(
            Os::Linux,
            env(&[("XDG_DATA_HOME", "relative"), ("HOME", "/tmp/home")])
        ),
        Some(PathBuf::from("/tmp/home/.local/share/otpkeep"))
    );
}

#[test]
fn the_override_wins_everywhere() {
    for os in [Os::MacOs, Os::Windows, Os::Linux] {
        let dir = data_dir_for(
            os,
            env(&[
                ("OTPKEEP_DATA_DIR", "/tmp/otpkeep-test/x"),
                ("HOME", "/tmp/home"),
            ]),
        );
        assert_eq!(dir, Some(PathBuf::from("/tmp/otpkeep-test/x")), "{os:?}");
    }
}

#[test]
fn no_home_means_no_directory() {
    assert_eq!(data_dir_for(Os::MacOs, env(&[])), None);
    assert_eq!(data_dir_for(Os::Windows, env(&[])), None);
}
