# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Warnings and errors are kept in a small log file (at most 1 MB plus one older file) to help with problem
  reports; it never holds secrets, codes or account names.
- When the data cannot be opened because another OTPKeep, the keychain or the disk is busy, the panels and the
  main window offer **Try again**; a damaged vault file can be put back to its previous version from the main
  window after a confirmation (the damaged file is kept).

### Changed

- Open windows use far less CPU: countdowns update once a second, on the second, and the rings empty by
  themselves instead of being redrawn on every tick; the main window draws only the accounts on screen, so a
  long list stays light.
- OTPKeep resting in the menu bar uses less memory: the search panel is opened when its shortcut is pressed and
  closed shortly after it hides, and the quick panel is closed after 10 minutes unused and opened again when
  asked for.

### Fixed

- Linux: the tray menu has **Show quick panel**, since clicking the tray icon there only opens the menu; the
  panel and the search panel open in the middle of the screen instead of with their corner there; and a panel
  closed through the window manager opens again next time.
- When the computer's clock is set back, the main window and panels fetch fresh codes instead of showing
  countdowns that never end.
- The menu bar icon's menu follows the language setting (it was always in Chinese) and changes with it.
- An account shortcut pressed before the master password was entered now opens the panel to ask for it,
  instead of doing nothing.
- A shortcut the system refused (usually because another app holds it) is no longer shown as if it worked;
  the account editor says it is not active.
- Typing a code from the search panel no longer hides an open main window on macOS (the code is copied
  instead), and when no key could be typed the panel comes back to say the code was copied instead.
- An import preview you leave open is forgotten, with its secrets, after 10 minutes or when the main window
  closes; accounts kept in the trash for 30 days are deleted even when OTPKeep runs for weeks without a
  restart.
- Entering the master password in one window, or generating a counter code, now updates the other open
  windows straight away.
- Copying the same code twice keeps it on the clipboard for the full time after the second copy, and a code
  still waiting to be cleared is cleared when OTPKeep quits.
- An export saved over an existing file is written completely before it replaces that file, and is readable
  only by you even when the old file was readable by others.
- A rename, edit or counter code made while a sync was running is no longer lost when the sync finishes, and an
  HOTP counter never goes back.

### Changed

- OTPKeep costs almost nothing while its windows are hidden: countdowns run only in a window on screen, which
  catches up when shown, and the main window is closed for good when you close it and made again when opened.
- Saving an encrypted backup no longer holds up the panels, shortcuts and sync while its key is derived.
- Entering the master password, saving, and importing pictures or backups no longer freeze the windows while
  they work; searching and changing many accounts at once are faster with large vaults.
- The main window saves an edited account in one write, and redraws only the rows whose countdown changed.

## [0.1.0] - 2026-10-25

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
