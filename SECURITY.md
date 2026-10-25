# Security Policy

OTPKeep stores two-factor secrets. We treat security reports as the highest priority.

## Supported versions

Until 1.0, only the latest release receives security fixes. After 1.0, the latest minor release of the
current major version is supported.

## Reporting a vulnerability

**Do not open a public issue.** Report privately through
[GitHub Security Advisories](https://github.com/weikxs/otpkeep/security/advisories/new).

Please include:

- the affected version (`otpkeep --version`) and platform;
- steps to reproduce, using demo data only — **never send real secrets, otpauth links, QR codes,
  backups or vault files**;
- the impact you expect (for example, a secret reaching a log file).

## What to expect

| Step | Target |
|---|---|
| Acknowledge the report | within 3 days |
| Initial assessment | within 7 days |
| Fix released for confirmed critical issues | as soon as possible, normally within 30 days |

We credit reporters in the release notes unless they prefer to stay anonymous.

## Design commitments

- The vault is encrypted on disk; its key is kept in the system keystore, or derived from a master
  password when no keystore is available. Secrets are never stored in plain text.
- OTPKeep reads only its own data. Another app's export is opened only when you choose the file, and only
  for reading.
- No telemetry. The app talks to the network only for the actions you start: a pairing or sync window on
  your local network, a time check, or an update check (both off by default).
- Pairing and sync happen only on the same local network, inside a short window you open, one device at a
  time, and every sync needs a one-time code and your approval on the other device.
- The local browser page and the optional local HTTP API listen on loopback only and require a token.

## Release packages and macOS

Every release publishes `checksums.txt` with the SHA-256 of each package; the install script refuses a package that
does not match it.

The macOS app is not signed with an Apple Developer ID or notarized by Apple, and there are no plans to sign it.
It is built by GitHub Actions from this repository's source, and `checksums.txt` lets you confirm a download is the
released file. The Homebrew cask and the install script check the checksum, then remove the quarantine attribute
from the installed `OTPKeep.app` (`xattr -dr com.apple.quarantine`); otherwise macOS would refuse to open it the
first time.

If you install the disk image by hand:

1. Check it: `shasum -a 256 otpkeep_<version>_darwin_universal.dmg` must match its line in `checksums.txt`.
2. Copy `OTPKeep.app` into Applications and open it. When macOS says it cannot verify the app, click **Done**.
3. In System Settings → Privacy & Security, under Security, click **Open Anyway** next to "OTPKeep was blocked",
   then open OTPKeep again and choose **Open**. On macOS 14 and earlier, Control-click the app and choose **Open**
   also works.

Instead of steps 2–3 you can run `xattr -dr com.apple.quarantine /Applications/OTPKeep.app`. macOS remembers the
choice until you replace the app by hand.
