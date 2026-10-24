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

The macOS app is not yet signed or notarized by Apple. The Homebrew cask and the install script therefore remove the
quarantine attribute from the installed `OTPKeep.app` (`xattr -dr com.apple.quarantine`); otherwise macOS would
refuse to open it the first time. If you prefer to keep Gatekeeper's check, download the disk image from the
releases page, verify it against `checksums.txt`, and choose **Open Anyway** in System Settings → Privacy &
Security when macOS asks.
