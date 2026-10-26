<div align="center">

<img alt="OTPKeep" src="apps/desktop-mobile/src-tauri/icons/128x128@2x.png" width="112">

# OTPKeep

**A free, private two-factor authenticator (TOTP / HOTP / Steam) that lives in your menu bar.**

Codes at hand with one click or one shortcut. Your accounts stay on your devices: no account, no cloud, no telemetry.

[简体中文](README.zh-CN.md) · [Features](#features) · [Install](#install) · [Quick start](#quick-start) · [Scripting](#scripting-and-cli) · [Security](#data-security-and-privacy)

[![Release](https://img.shields.io/github/v/release/weikxs/otpkeep)](https://github.com/weikxs/otpkeep/releases/latest)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux-lightgrey)
![Rust](https://img.shields.io/badge/rust-1.95-b7410e)
![Tauri](https://img.shields.io/badge/tauri-2-24c8db)
![License](https://img.shields.io/badge/license-not%20chosen%20yet-lightgrey)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/en/main-dark.png">
  <img alt="The OTPKeep main window" src="docs/images/en/main-light.png" width="860">
</picture>

</div>

> [!IMPORTANT]
> OTPKeep 0.1.0 is the first release. Until you trust it, keep another copy of your two-factor accounts — an
> encrypted backup is one click away (**Export → Encrypted backup**).

## Contents

- [Why OTPKeep](#why-otpkeep)
- [Features](#features)
- [Comparison](#comparison)
- [Performance](#performance)
- [Screenshots](#screenshots)
- [Install](#install) · [Uninstall](#uninstall)
- [Quick start](#quick-start)
- [Usage](#usage)
- [Scripting and CLI](#scripting-and-cli)
- [Data, security and privacy](#data-security-and-privacy)
- [FAQ and troubleshooting](#faq-and-troubleshooting)
- [Development](#development) · [Contributing](#contributing) · [Security policy](#security-policy) · [License](#license)

## Why OTPKeep

- **Sync without servers.** Pair your own devices once, then sync them end to end encrypted over the same local
  network or Bluetooth, inside a short window you open. No account, no cloud.
- **A native menu bar menu.** One click on the icon shows your recently used accounts with their current codes, in
  the system's own menu; choose one to copy its code.
- **Finds what you mean.** Search names, services, accounts, tags and notes, including Chinese pinyin and initials
  (`yh` finds 银行).
- **Works with your scripts.** `otpkeep code github --json` prints a code, also while the app is running.
- **Light on your computer.** No web page runs while OTPKeep waits in the menu bar; see [Performance](#performance).

## Features

### Codes at hand

- **Menu bar / tray menu**: your recently used accounts, then the others, with their current codes; choose one to
  copy its code. It is the system's own menu, so OTPKeep runs no web page while it waits in the menu bar.
- **Global shortcuts**: `⌃⌥⌘1` … `⌃⌥⌘0` copy the first ten accounts (macOS); any account can have its own shortcut.
- **Search panel** (`⌃⌥⌘Space`): type a name, press Enter to copy, `⇧Enter` to **type the code** into the app you
  are using.
- **Clipboard care**: copied codes are kept out of clipboard history apps and cleared again after 30 seconds
  (adjustable).

### Search

- Names, services, accounts, tags and notes, including Chinese pinyin and initials (`yh` finds 银行).

### Organising

- Pin, drag to reorder, tags with filters, notes, a 30-day trash and an edit history per account.
- **Browser management page**: rename accounts, write notes, set shortcuts and copy codes from a page OTPKeep
  serves on `127.0.0.1` only, opened with a one-time link.

### Import and export

- **Adds anything common**: `otpauth://` links (one or many), QR code images (also pasted from the clipboard),
  Google Authenticator transfer codes, Steam Guard, OTPKeep backups and GoldenPassport exports. Links with small
  mistakes (spaces, lower case, a missing issuer) are repaired; Microsoft and Oracle proprietary codes are
  recognised and explained instead of failing silently.
- **Exports anywhere**: an encrypted OTPKeep backup, `otpauth://` links, Google Authenticator transfer QR codes or
  a GoldenPassport file.

### Sync

- **Local-network sync, no servers**: pair your devices once, then sync them end to end encrypted over the same
  local network or Bluetooth, inside a short window you open, with a one-time code and your approval on the other
  device.

### Scripting and command line

- `otpkeep code github` prints a code; `--json` for programs. It works while the app runs too (macOS, Linux).
  See [Scripting and CLI](#scripting-and-cli).

### Privacy and looks

- **Private by design**: the data is encrypted on disk with a key kept in the system keychain, or derived from an
  optional master password. No telemetry, no accounts, no cloud.
- **Looks at home**: light and dark themes, English and Simplified Chinese, the system's own menu in the menu bar
  and, on macOS, the system's own password dialog.

### Platforms

| Platform | State |
|---|---|
| macOS 11+ | Released: one universal app for Apple silicon and Intel — menu bar menu, main window, search panel, typing codes, browser management page, import / export, local-network sync |
| Linux (x86_64, arm64) | Released: `.deb`, `.rpm` and AppImage; typing codes works on X11, not on Wayland |
| Windows | Planned |
| Android (APK) | Planned |
| iOS | Planned |
| Command line (`otpkeep`) | Working on all three desktop systems |

## Comparison

| | Desktop app | Menu bar / tray | Sync without an account or cloud | Cloud sync | Encrypted backup export | Imports Google transfer codes | Source code | Price |
|---|---|---|---|---|---|---|---|---|
| **OTPKeep** | macOS, Linux (Windows planned) | Yes | Yes (local network, Bluetooth) | No | Yes | Yes | Public; license not chosen yet | Free |
| GoldenPassport | macOS | Yes | — | — | — | — | — | — |
| Google Authenticator | No (mobile) | No | No | Yes (Google account) | No | Yes | Closed | Free |
| Microsoft Authenticator | No (mobile) | No | No | Yes (Microsoft account) | No | — | Closed | Free |
| Authy | No (desktop apps discontinued in 2024) | No | No | Yes (Authy account) | No | — | Closed | Free |
| Ente Auth | macOS, Windows, Linux | — | No | Yes (end-to-end encrypted, Ente account) | Yes | Yes | Open source | Free |
| 2FAS | — (mobile apps and a browser extension) | — | — | Yes (Google Drive / iCloud backup) | Yes | Yes | Open source | Free |
| Aegis | No (Android) | No | No | No | Yes | Yes | Open source | Free |

"—" means we could not confirm it. Checked against each product's public documentation as known in 2025;
corrections are welcome in an issue.

## Performance

<!-- PERFORMANCE: filled in by measurement -->

| Scenario | App | Memory footprint | CPU | Processes |
|---|---|---|---|---|
| Menu bar idle, keychain mode, locked | OTPKeep | _to be measured_ | _to be measured_ | _to be measured_ |
| | GoldenPassport | _to be measured_ | _to be measured_ | _to be measured_ |
| Menu bar idle, unlocked, using only the menu | OTPKeep | _to be measured_ | _to be measured_ | _to be measured_ |
| | GoldenPassport | _to be measured_ | _to be measured_ | _to be measured_ |
| After opening and closing the main window | OTPKeep | _to be measured_ | _to be measured_ | _to be measured_ |
| | GoldenPassport | _to be measured_ | _to be measured_ | _to be measured_ |

**Method.** Both apps on the same Mac (_model and macOS version to be filled in_), with 200 demo accounts in
OTPKeep and 8 in GoldenPassport. Memory is the physical footprint reported by `vmmap --summary <pid>`, summed over
the app's own processes; CPU is the average over 60 seconds of `ps` CPU time. To reproduce, start OTPKeep with a
throwaway data folder (`OTPKEEP_DATA_DIR=/tmp/otpkeep-demo OTPKEEP_KEYSTORE=memory`), import demo accounts with
`otpkeep import`, leave it in the menu bar for a minute, and run `vmmap --summary` on its process and on any
`com.apple.WebKit.*` processes it started.

<!-- /PERFORMANCE -->

## Screenshots

<table>
  <tr>
    <td align="center" colspan="2"><img alt="Search panel" src="docs/images/en/search.png" width="560"><br><sub>Search panel: Enter copies, ⇧Enter types the code, ⌘E edits</sub></td>
  </tr>
  <tr>
    <td align="center" colspan="2"><img alt="Browser management page" src="docs/images/en/gate.png" width="860"><br><sub>Browser management page: names, notes, shortcuts and codes</sub></td>
  </tr>
  <tr>
    <td align="center"><img alt="Adding accounts" src="docs/images/en/add.png" width="420"><br><sub>Adding accounts: check the codes before saving</sub></td>
    <td align="center"><img alt="Sync" src="docs/images/en/sync.png" width="420"><br><sub>Sync between your own devices</sub></td>
  </tr>
  <tr>
    <td align="center" colspan="2"><img alt="Settings" src="docs/images/en/settings.png" width="560"><br><sub>Settings</sub></td>
  </tr>
</table>

A picture of the menu bar menu (`docs/images/en/tray-menu.png`) is coming: the system draws the menu, so it cannot be
made by the browser tests. Screenshots show made-up demo accounts. They are generated from `apps/desktop-mobile`
with `README_SHOTS=../../docs/images pnpm e2e readme-shots`.

## Install

### macOS and Linux, fully automatic

```sh
curl -fsSL https://raw.githubusercontent.com/weikxs/otpkeep/main/install.sh | sh
```

The script picks the package for your system and CPU, checks it against the release's `checksums.txt` and
installs it — nothing else is changed:

| System | What it installs |
|---|---|
| macOS | `OTPKeep.app` into `/Applications` (or `~/Applications`), without the "downloaded from the internet" prompt |
| Debian, Ubuntu, Raspberry Pi OS | the `.deb`, with `apt` (asks for `sudo`) |
| Fedora, RHEL, openSUSE (x86_64) | the `.rpm`, with `dnf`, `zypper` or `yum` (asks for `sudo`) |
| Any other Linux | the AppImage, as `otpkeep-app` in `~/.local/bin`, with an applications-menu entry |

Run it again to upgrade. Options go after `sh -s --`, for example
`curl -fsSL https://raw.githubusercontent.com/weikxs/otpkeep/main/install.sh | sh -s -- --version v0.1.0`:

| Option | Meaning |
|---|---|
| `--version <tag>` | install this release instead of the latest |
| `--format deb\|rpm\|appimage` | Linux: choose the package format |
| `--dir <folder>` | macOS: where `OTPKeep.app` goes; AppImage: where `otpkeep-app` goes |
| `--no-modify-path` | AppImage: do not add the folder to your shell's `PATH` |

### Homebrew (macOS)

```sh
brew install --cask weikxs/tap/otpkeep
brew upgrade --cask otpkeep          # later, to upgrade
```

### Packages

Download from the [latest release](https://github.com/weikxs/otpkeep/releases/latest) and check the file against
`checksums.txt` (`shasum -a 256 -c checksums.txt --ignore-missing`):

| File | Platform |
|---|---|
| `otpkeep_<version>_darwin_universal.dmg` | macOS 11+, Apple silicon and Intel |
| `otpkeep_<version>_linux_amd64.deb` / `_arm64.deb` | Debian, Ubuntu, Raspberry Pi OS (x86_64 / arm64) |
| `otpkeep_<version>_linux_amd64.rpm` | Fedora, RHEL, openSUSE (x86_64) |
| `otpkeep_<version>_linux_amd64.AppImage` / `_arm64.AppImage` | Any Linux with glibc (x86_64 / arm64) |

The macOS app is not signed or notarized by Apple, and we do not plan to sign it. The script and Homebrew handle
that for you. If you install the disk image yourself, check it against `checksums.txt`, open OTPKeep once, then
choose **Open Anyway** in System Settings → Privacy & Security and open it again — or run
`xattr -dr com.apple.quarantine /Applications/OTPKeep.app`. Details in [SECURITY.md](SECURITY.md#release-packages-and-macos).

### Building from source

| Tool | Version |
|---|---|
| Rust | 1.95 (installed automatically from `rust-toolchain.toml` by `rustup`) |
| Node.js | a current LTS release |
| pnpm | 11 (`corepack enable`) |
| Tauri system dependencies | see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (Xcode Command Line Tools on macOS, WebView2 on Windows, WebKitGTK on Linux) |

```sh
git clone https://github.com/weikxs/otpkeep.git
cd otpkeep

make app                      # the desktop app (debug build)
cargo build -p otpkeep-cli    # the otpkeep command → target/debug/otpkeep
```

`make app` leaves the app bundle under `target/debug/bundle/` (on macOS `OTPKeep.app`).

To try the app without touching your real data or keychain, start it in development mode with a throwaway
data folder and an in-memory keystore:

```sh
cd apps/desktop-mobile
OTPKEEP_DATA_DIR=/tmp/otpkeep-demo OTPKEEP_KEYSTORE=memory pnpm tauri dev
```

### Uninstall

| Installed with | Remove with |
|---|---|
| The script or the disk image (macOS) | Quit OTPKeep (menu bar icon → Quit), then move `OTPKeep.app` to the Trash |
| Homebrew | `brew uninstall --cask otpkeep` |
| `.deb` | `sudo apt remove otp-keep` |
| `.rpm` | `sudo dnf remove otp-keep` (or `sudo zypper remove otp-keep`) |
| AppImage | `rm ~/.local/bin/otpkeep-app ~/.local/share/applications/otpkeep.desktop ~/.local/share/icons/hicolor/256x256/apps/otpkeep.png` |

Uninstalling never touches your accounts: they stay in OTPKeep's [data folder](#data-security-and-privacy). To
remove them too, export a backup first if you may need them, then delete that folder.

## Quick start

1. **Start OTPKeep.** It lives in the **menu bar** (macOS) or the **system tray** (Windows, Linux); it has no Dock
   icon. Click the icon to open its menu and choose **Open OTPKeep**.
2. **Add your accounts**: paste a link, drop or paste a QR code image, or import a file — a Google Authenticator
   transfer code, an OTPKeep backup or a GoldenPassport export. Every import shows a preview with the current codes
   first, so you can compare them with the website before anything is saved.
3. **Get a code**: click the menu bar icon and choose the account, press `⌃⌥⌘Space` and type its name, or run
   `otpkeep code <name>`. The code is copied and cleared from the clipboard again after 30 seconds.

## Usage

### The menu bar menu

A click on the icon opens a menu drawn by the system (no web page runs for it):

- **Search…** at the top opens the search panel (its shortcut, `⌃⌥⌘Space` on macOS, is shown beside it).
- Then 30 accounts (10–50, set in Settings): the ones whose codes you copied last come first, the rest follow in the main
  window's order, pinned ones first. Each reads `123 456   Name · Note`, the code first so the codes line up. Without a note
  the account name stands in, unless the name already shows it; an imported name like `Example Cloud:ops@example.com` is
  shown as `Example Cloud`. Names and notes are shortened to about 20–24 characters, and the account's own shortcut is shown
  beside it, if it has one.
- **More accounts (N)** is a submenu with every other account in the main window's order; it scrolls. Above 1000 accounts it says
  how many were left out, and the search panel finds them.
- Then **Open OTPKeep** (the main window), **Manage in the browser…**, **Sync…**, **Settings…** and **Quit OTPKeep**.

Choose an account to copy its code; the clipboard is cleared as after any copy. Counter-based (HOTP) accounts read
**(press for code)** and make a new code when chosen. The last 10 accounts whose codes you copied (from the menu, a shortcut,
the search panel, the main window or the management page) are remembered as ids only, in this device's settings file; they are
never synced.

The codes are brought up to date when they change, when your accounts change and when the pointer reaches the icon.
With a master password, the menu offers **Unlock…** until you enter it. On macOS the system's own password dialog asks for it
(no web page is started, so no WebKit or AutoFill helper runs); on Windows and Linux a small window asks and closes once the
data is open. A shortcut pressed before that asks the same way. When the data cannot be opened, the menu says why and offers
**Try again** or the main window.

The main window and the search panel are closed for good 30 seconds after you close them, so while OTPKeep waits in the menu bar
no web page runs.

### Keyboard shortcuts

| Where | Keys | Action |
|---|---|---|
| Anywhere (macOS default) | `⌃⌥⌘1` … `⌃⌥⌘0` | Copy the code of account 1–10 in the list |
| Anywhere (macOS default) | `⌃⌥⌘Space` | Open the search panel |
| Anywhere | your choice | Copy one account's code (set per account) |
| Search panel | `Enter` / `⇧Enter` | Copy / type the code |
| Search panel | `⌘E`, `Esc` | Edit in the main window, clear then close |

On Windows and Linux the global shortcuts start turned off; choose them in Settings. Combinations with only `Shift` are refused, so typing is never captured.

### Search panel and typing codes

Press `⌃⌥⌘Space` anywhere (macOS default; set or turn it off in Settings). Type part of a name, account, note or tag:

- `Enter` copies the code and closes.
- `⇧Enter` types the code into the field you were in. On macOS this needs **Accessibility** permission
  (System Settings → Privacy & Security → Accessibility); without it, on Wayland, while the main window is open (macOS), or
  when no key could be sent, the code is copied instead and the search panel says so.
- `⌘E` (`Ctrl+E`) opens the account in the main window; `Esc` clears the search, then closes.

### Main window

Lists every account with its code. Click a row to copy. **Edit** opens the account's names, note, tags, pin, place in the list,
code settings, own shortcut, QR code and history. **Select** chooses several accounts to tag, export or move to the trash at once.

### Counter-based (HOTP) accounts

Time-based codes change every 30 seconds by themselves. Counter-based (HOTP) codes, used by some bank tokens, move a counter forward each
time one is made, so OTPKeep does not show one until you press **Generate**; otherwise every glance would use up a code.

### Browser management page

Choose **Manage in the browser** from the menu. OTPKeep serves the page on `127.0.0.1` with a one-time link; the session ends when the tab closes or after 45 seconds without it.
You can rename accounts, write notes, set or clear each account's shortcut, and copy codes (OTPKeep writes the clipboard itself).

### Importing and exporting

| Import | Notes |
|---|---|
| `otpauth://totp/…`, `otpauth://hotp/…`, `otpauth://steam/…` | one or many, also inside other text |
| QR code images | files, drag and drop, or a pasted screenshot; several codes in one image |
| Google Authenticator transfer codes | `otpauth-migration://` links or their QR codes |
| OTPKeep backup | encrypted with a backup password |
| GoldenPassport export | `.secrets` or `accounts.json` |

Every import shows a preview with the current codes, so you can compare them with the website before anything is saved; accounts you already have are recognised.

Export from the main window (all accounts or the selected ones) as an **encrypted backup** (recommended), `otpauth://` links,
Google Authenticator transfer QR codes, or a GoldenPassport file. Every format except the backup is unencrypted, and OTPKeep asks before writing one.

### Sync between your devices

1. On the first device: **Sync → Add a device**. It shows an eight-digit pairing code (and a QR invite).
2. On the new device: **Sync → Join an existing sync group**, pick the waiting device and enter the code.
3. Compare the six-digit check code on both screens and confirm.

Later syncs: open a **sync window** on one device and enter its code on the other; the first device asks you to allow it.
Sync works only between devices near each other, over the same local network or Bluetooth, while a window is open (1–5 minutes). There is no server.

### Settings

Language (system, English, 中文), theme, clearing the clipboard after copying (default 30 seconds), shortcuts for the first ten accounts,
the search panel shortcut, how many accounts the menu bar menu shows, whether scripts may get codes from the running app,
and typing permission.

## Scripting and CLI

```console
$ otpkeep code github          # the code of the one account matching "github"
$ otpkeep list --tag work      # names only, never codes
$ otpkeep import accounts.txt  # links, QR images, backups, GoldenPassport exports
$ otpkeep export --format backup --out otpkeep.backup
$ otpkeep sync join            # sync with a device that opened a sync window
```

Run `otpkeep help` for every command (`init`, `import`, `export`, `sync wait|join`, `pair open|join`, `devices`, `group leave`, `rename`, …).
Every vault command accepts `--json` for machine-readable output and `--password-stdin` for a master password from standard input.

`otpkeep code` works while the app runs too (macOS and Linux): the app answers through a private local socket
instead of the command line waiting for the data. Settings → **Allow scripts to get codes from the running app**
controls it; it is on by default without a master password and off (opt-in) with one, since scripts would then
get codes without the password. Codes given to scripts do not count as recently used.

```sh
otpkeep code github --json | jq -r .data.code
```

```python
import json, subprocess
reply = json.loads(subprocess.run(["otpkeep", "code", "github", "--json"], capture_output=True, text=True).stdout)
print(reply["data"]["code"] if reply["ok"] else reply["error"]["code"])
```

Matching rules, every exit and error code, Node.js, fish, Shortcuts / Raycast / Alfred examples, the socket
protocol and the security notes are in **[Using OTPKeep from scripts](docs/scripting.md)**.

## Data, security and privacy

| | |
|---|---|
| **Where** | macOS `~/Library/Application Support/OTPKeep` · Windows `%LOCALAPPDATA%\OTPKeep` · Linux `$XDG_DATA_HOME/otpkeep` (default `~/.local/share/otpkeep`); `OTPKEEP_DATA_DIR` overrides it |
| **Encryption at rest** | XChaCha20-Poly1305; the key is kept in the system keychain (Keychain, Credential Manager, Secret Service), or derived from an optional master password with Argon2id |
| **Backups** | encrypted with a separate backup password |
| **Sync** | Noise XX (X25519, ChaCha20-Poly1305, BLAKE2s) between Ed25519 device identities; the one-time code is checked with SPAKE2, so a recorded session cannot be brute-forced offline |
| **Clipboard** | copied codes are kept out of clipboard history apps and cleared 30 seconds (adjustable) after the last copy, or when OTPKeep quits, only if the clipboard still holds that code |
| **Network** | none, except local-network / Bluetooth sync while you have a window open, and the management page on `127.0.0.1` |
| **Scripts** | a local socket in the data folder, readable only by you, when allowed in Settings ([details](docs/scripting.md#security)) |
| **Telemetry** | none |
| **Log** | warnings and errors only, never secrets, codes or account names, in `otpkeep.log` (at most 1 MB, plus one older file): macOS `~/Library/Logs/io.github.weikxs.otpkeep`, elsewhere the `logs` folder under `io.github.weikxs.otpkeep` in the local app data folder |

There is deliberately **no app lock**: the master password (optional) is asked once per start, and the system's own screen lock protects an unattended computer.
The app never reads keys, passwords or keychain items other than its own.

## FAQ and troubleshooting

**Why does an account show "Generate" instead of a code?** It is a counter-based (HOTP) account; see [above](#counter-based-hotp-accounts).

**Can I sync through iCloud, Dropbox or a server?** No. Sync happens only directly between your devices, near each other, when you open a window. Use encrypted backups to keep a copy elsewhere.

**Is there a Mac App Store version?** No. OTPKeep is distributed through its releases, the install script and Homebrew.

**Why is the Linux package called `otp-keep`?** The package tools derive it from the app's name; the program itself
is `otpkeep-app`, and the command line is `otpkeep`.

**Typing codes does nothing.** Grant Accessibility permission (macOS). On Wayland, typing is not possible and the code is copied instead.

**`otpkeep code` says `DISABLED` or `BUSY`.** The app is running and does not allow scripts (turn it on in Settings), or
it runs on Windows, where scripts cannot ask it yet. See [troubleshooting for scripts](docs/scripting.md#troubleshooting).

**macOS says the app cannot be opened.** It is not signed by Apple; see [Packages](#packages).

## Development

```sh
make check      # all checks: privacy, format, clippy, tests, coverage, cargo-deny, audit, performance, simulation
make test       # Rust and UI tests
make fmt        # format the Rust code
make hooks      # use the repository's git hooks
cd apps/desktop-mobile && pnpm test     # UI unit tests (Vitest)
cd apps/desktop-mobile && pnpm e2e      # browser tests (Playwright)
```

Tests write only under `/tmp/otpkeep-test` and use demo data. See [CONTRIBUTING.md](CONTRIBUTING.md) and the [development standards](docs/development/standards.md).

### Project structure

```
apps/
  cli/              the otpkeep command
  desktop-mobile/   the Tauri app: Rust shell (src-tauri) and Preact interface (src)
crates/
  keep-passcode     HOTP, TOTP and Steam codes, Base32
  keep-keyuri       otpauth:// links: parsing, repairing, writing
  keep-vault        the encrypted vault: entries, keys, storage, locking, migration
  keep-find         search with pinyin and initials
  keep-transfer     backups, Google Authenticator transfer codes, QR codes, GoldenPassport files
  keep-platform     keychain, data folders, the scripts' socket and other platform services
  keep-sync         pairing and sync: windows, handshakes, membership, merging
  keep-link         reaching another device over the local network or Bluetooth
  keep-ble          Bluetooth Low Energy transport
  keep-gate         the browser management page server
  keep-ffi          reserved for Swift and Kotlin bindings
docs/               development standards, the scripting guide and images
fuzz/               fuzz targets for the parsers
```

## Contributing

Issues and pull requests are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the [code of conduct](CODE_OF_CONDUCT.md) first.
Notable changes are recorded in the [changelog](CHANGELOG.md).

## Security policy

Please report vulnerabilities privately as described in [SECURITY.md](SECURITY.md), not in public issues.

## License

Not chosen yet. Until a license file is added, all rights are reserved.
