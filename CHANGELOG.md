# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - TBD

The first public release, for macOS and Linux.

### Added

- Menu bar / tray quick panel: codes with countdown rings, the next code in the last five seconds, pinned and
  all accounts, a translucent window with adjustable opacity.
- Global shortcuts: the first ten accounts (`⌃⌥⌘1` … `⌃⌥⌘0` on macOS) and a shortcut per account, kept on
  this device only.
- Search panel (`⌃⌥⌘Space`): Enter copies, `⇧Enter` types the code into the focused field (macOS
  Accessibility, Linux X11), `⌘E` edits.
- Main window: list in one or two columns, search with pinyin, tags, pins, drag-to-reorder, 30-day trash, edit
  history, code settings, per-account shortcuts, batch actions and settings.
- Adding accounts: `otpauth://` links (forgiving of common mistakes), QR code images and pasted screenshots,
  Google Authenticator transfer codes, OTPKeep backups and GoldenPassport exports, with a preview of the
  current codes; Microsoft and Oracle proprietary codes are recognised and explained.
- Steam Guard codes alongside TOTP and HOTP.
- Export: encrypted backup, `otpauth://` links, Google Authenticator transfer QR codes, GoldenPassport files.
- Browser management page on `127.0.0.1` with a one-time link: names, notes, shortcuts, codes and copying.
- Pairing and sync between your own devices over the local network or Bluetooth, inside a window you open,
  with a one-time code and approval on the other device.
- The `otpkeep` command line: codes, lists, import, export, pairing and sync, with `--json`.
- English and Simplified Chinese on every page, light and dark themes, an app icon.
- Installers: a universal macOS disk image; `.deb`, `.rpm` and AppImage for Linux; an install script and a
  Homebrew cask.

[Unreleased]: https://github.com/weikxs/otpkeep/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/weikxs/otpkeep/releases/tag/v0.1.0
