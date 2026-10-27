# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `otpkeep code` works while the app runs (macOS, Linux): the app answers through a private local socket in the
  data folder instead of the command line waiting 5 seconds and failing with `BUSY`. A new setting, **Allow
  scripts to get codes from the running app**, is on by default without a master password and off (opt-in)
  with one; when off, the command line answers `DISABLED` (exit code 12). Codes given to scripts do not count
  as recently used. See [Using OTPKeep from scripts](docs/scripting.md).
- Warnings and errors are kept in a small log file (at most 1 MB plus one older file) to help with problem
  reports; it never holds secrets, codes or account names.
- When the data cannot be opened because another OTPKeep, the keychain or the disk is busy, the panels and the
  main window offer **Try again**; a damaged vault file can be put back to its previous version from the main
  window after a confirmation (the damaged file is kept).

### Changed

- Open windows use far less CPU: countdowns update once a second, on the second, and the rings empty by
  themselves instead of being redrawn on every tick; the main window and the panels draw only the accounts on
  screen, so a long list stays light.
- The menu bar / tray icon now opens the system's own menu instead of the quick panel: **Search…** (with the
  search panel's shortcut), then 30 accounts (10–50, a new setting), the most recently used first (the last 10
  accounts whose codes you copied, remembered as ids in this device's settings and never synced) and the rest in
  the main window's order, then **More accounts (N)** with every other account, then **Open OTPKeep**,
  **Manage in the browser…**, **Sync…**, **Settings…** and **Quit OTPKeep**. Each account shows its current code first (so
  the codes line up), its name and its note or account name, without repeating what the name already says,
  and its own shortcut; choosing one copies the code, and
  counter-based accounts make a new code when chosen. The codes are brought up to date when they change, when the
  accounts change and when the pointer reaches the icon.
- With a master password, the menu offers **Unlock…**. On macOS it asks with the system's own password dialog, so
  unlocking starts no web page (no WebKit or AutoFill helper); on Windows and Linux a small password window
  asks and closes once the data is open. A shortcut pressed before the password was entered asks the same way.
- OTPKeep resting in the menu bar runs no web page: the search panel and the main window are made when asked
  for and closed for good 30 seconds after they are closed; OTPKeep keeps running in the menu bar with no
  window open.

### Fixed

- macOS: unlocking with a master password no longer leaves the app holding about 64 MB more memory for minutes:
  the memory used to derive the key is given back to the system at once.
- An imported name like `Example Cloud:ops@example.com` whose part after the colon is the account is saved as
  `Example Cloud` (and as the service when it had none), so the account is no longer shown twice. Accounts
  already in the vault are not changed, but the menu, the main window and the search panel show them the same
  short way.
- Linux: the search panel opens in the middle of the screen instead of with its corner there, and opens again
  after it was closed through the window manager.
- When the computer's clock is set back, the main window and panels fetch fresh codes instead of showing
  countdowns that never end.
- The menu bar icon's menu follows the language setting (it was always in Chinese) and changes with it.
- An account shortcut pressed before the master password was entered now asks for it, instead of doing
  nothing.
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
  catches up when shown.
- Saving an encrypted backup no longer holds up the panels, shortcuts and sync while its key is derived.
- Entering the master password, saving, and importing pictures or backups no longer freeze the windows while
  they work; searching and changing many accounts at once are faster with large vaults.
- The main window saves an edited account in one write, and redraws only the rows whose countdown changed.

### Removed

- The quick panel and its **Menu bar panel opacity** setting, replaced by the menu bar menu. OTPKeep no longer
  needs macOS private API.

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
