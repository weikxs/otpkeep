//! Embeds `Info.plist` in the macOS binary: macOS requires a Bluetooth usage description before a program may
//! use Bluetooth, and a command-line tool has no bundle to carry one.

fn main() {
    println!("cargo:rerun-if-changed=Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Info.plist");
        println!(
            "cargo:rustc-link-arg-bin=otpkeep=-Wl,-sectcreate,__TEXT,__info_plist,{}",
            plist.display()
        );
    }
}
