<!--
  Release notes template. It has the layout of the v0.1.0 release
  (https://github.com/weikxs/otpkeep/releases/tag/v0.1.0, file v0.1.0.md);
  keep every release in the same order and voice.

  One file per release: .github/release-notes/vX.Y.Z.md (a pre-release such as
  vX.Y.Z-rc.1 has its own file too). The release workflow publishes it, word
  for word, as the GitHub release page, and stops when the file is missing or
  still contains TODO. Draft one from the changelog:

      scripts/release-notes.sh vX.Y.Z > .github/release-notes/vX.Y.Z.md

  Voice: product communication for people deciding whether to install or
  upgrade, not a developer changelog (CHANGELOG.md is that). Speak to "you",
  say what the reader notices, not how it was built. No commit hashes, author
  emails, real names, companies or local paths; examples use the demo
  accounts of docs/development/standards.md, never real secrets.

  Sections marked "(only when …)" are left out when they have nothing to say.
  Every other section is always there. Delete these comments when done.
-->

# OTPKeep vX.Y.Z — One-line theme

Released on `YYYY-MM-DD`.

> OTPKeep is a free, private authenticator for TOTP, HOTP and Steam codes that lives in your menu bar: a click, a
> shortcut or a few typed letters give you the code, and your accounts never leave your own devices.

## Summary

<!-- Two or three sentences: what this release is, what changes for you, which systems it runs on. -->

## Who this release is for

<!--
  One or two sentences naming the person it helps most, specifically, then an
  "If you have ever:" list of 3–5 real moments, then one sentence on how this
  release changes them. As in v0.1.0:

  You sign in with two-factor codes many times a day, on the computer in front of you — ...

  If you have ever:

  - reached for your phone, unlocked it and scrolled for the right account while the login page timed out
  - ...

  then OTPKeep ...
-->

## Before you upgrade

<!-- (only when you must act) Changes to the data, the sync protocol or shortcuts, steps to run after upgrading, in order. -->

## What you can do now

<!--
  3–6 short paragraphs, one per capability. Each starts with a bold verb
  phrase and ends with the moment you notice it. As in v0.1.0:

  **Copy a code without looking for it.** Click the menu bar icon and click the account, ...
-->

## What's new

<!--
  Grouped by theme, one "###" per theme (v0.1.0: Codes at hand, Adding and
  moving accounts, Managing accounts, Sync between your devices, Look and
  feel). Each item: a bold lead, what it does, and its boundary (what it does
  not do).
-->

### Theme

- **Lead.** What it does, and what it does not do.

## Bug fixes

<!-- (only when there are fixes) One line each, in the reader's words: what went wrong, and that it no longer does. -->

## Privacy and security

<!--
  Always present, even when nothing changed. Keep the five promises, then add
  this release's security fixes with their severity (and advisory ID).
-->

- **Encrypted on your device.** Accounts are stored with XChaCha20-Poly1305; the key lives in the system keychain, or
  is derived from an optional master password with Argon2id.
- **No account, no server, no telemetry.** OTPKeep uses the network only for sync with your own devices nearby while
  a window is open, and for the management page on `127.0.0.1`.
- **Sync you can trust.** The Noise protocol between your devices' Ed25519 identities; the one-time code is checked
  with SPAKE2, so a recorded session cannot be guessed offline.
- **Careful with the clipboard.** Copied codes are kept out of clipboard history apps and cleared after 30 seconds
  (adjustable), only if the clipboard still holds that code.
- **Never reads what is not its own.** OTPKeep never opens keys, passwords or keychain items other than its own.

## Install

- Script (macOS and Linux, fully automatic): `curl -fsSL https://raw.githubusercontent.com/weikxs/otpkeep/main/install.sh | sh`
- Homebrew (macOS): `brew install --cask weikxs/tap/otpkeep`, or `brew upgrade --cask otpkeep`
- A package below; check it against `checksums.txt`

| File | Platform |
|---|---|
| `otpkeep_X.Y.Z_darwin_universal.dmg` | macOS, Apple silicon and Intel |
| `otpkeep_X.Y.Z_linux_amd64.deb` / `_arm64.deb` | Debian, Ubuntu, Raspberry Pi OS (x86_64 / arm64) |
| `otpkeep_X.Y.Z_linux_amd64.rpm` | Fedora, RHEL, openSUSE (x86_64) |
| `otpkeep_X.Y.Z_linux_amd64.AppImage` / `_arm64.AppImage` | Any Linux with glibc (x86_64 / arm64) |

Then open OTPKeep — it appears in the menu bar (macOS) or the system tray (Linux) — and add your first account.

## Compatibility

- macOS 11 (Big Sur) or newer on Apple silicon and Intel.
- Linux with glibc on x86_64 and arm64: Ubuntu 22.04+, Debian 12+, Fedora 38+ and their relatives; other
  distributions through the AppImage. Typing codes works on X11, not on Wayland.
- English and Simplified Chinese; other system languages show English.
- Windows, Android and iOS are planned.

## What's coming next

<!-- (only when there is a plan to share) One or two themes of the next release, without dates. -->

## Notes

- The macOS app is not signed or notarized by Apple; Homebrew and the install script remove the quarantine flag. By
  hand: **Open Anyway** in System Settings → Privacy & Security. See
  [SECURITY.md](https://github.com/weikxs/otpkeep/blob/vX.Y.Z/SECURITY.md).
- **Full changelog:** [vA.B.C...vX.Y.Z](https://github.com/weikxs/otpkeep/compare/vA.B.C...vX.Y.Z) ·
  [CHANGELOG.md](https://github.com/weikxs/otpkeep/blob/vX.Y.Z/CHANGELOG.md)
- Documentation: [README](https://github.com/weikxs/otpkeep#readme) ·
  [简体中文](https://github.com/weikxs/otpkeep/blob/main/README.zh-CN.md)
- Feedback and problems: [issues](https://github.com/weikxs/otpkeep/issues).
