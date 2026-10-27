# Development Standards

These rules apply to every change, by anyone, including the maintainers. **MUST** rules are enforced by tooling
or review; **SHOULD** rules need a reason in the pull request when broken. Where a tool enforces a rule, the
rule says which; "review" means a maintainer checks it.

They follow the practice of large open-source projects: issue-first contribution (GitHub CLI, uv, Terraform),
human responsibility for AI-assisted work (Terraform, uv, curl), focused commits and squash rules (Kubernetes,
curl), structured bug reports (Go), protected branches and required checks (Google, Microsoft, Kubernetes), and
the [OpenSSF Scorecard](https://scorecard.dev) checks for the supply chain.

## 1. Standards we follow

| Area | Standard |
|---|---|
| Versioning | [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) |
| Commit messages | [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) |
| Changelog | [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) |
| Rust style | [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/), `rustfmt`, `clippy` (pedantic) with `-D warnings` |
| TypeScript style | `strict` mode, checked by `tsc` (`pnpm typecheck`) |
| Shell style | POSIX `sh`, clean under [ShellCheck](https://www.shellcheck.net) |
| Workflows | Clean under [actionlint](https://github.com/rhysd/actionlint) |
| One-time passwords | [RFC 4226](https://www.rfc-editor.org/rfc/rfc4226) (HOTP), [RFC 6238](https://www.rfc-editor.org/rfc/rfc6238) (TOTP), [RFC 4648](https://www.rfc-editor.org/rfc/rfc4648) (Base32) |
| User interface | [WCAG 2.2](https://www.w3.org/TR/WCAG22/) level AA |
| CLI behavior | [Command Line Interface Guidelines](https://clig.dev/) |
| Community | [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) |
| Security reporting | Private GitHub Security Advisories ([SECURITY.md](../../SECURITY.md)) |
| Supply chain | [OpenSSF Scorecard](https://scorecard.dev) checks |
| License | Not chosen yet; all rights reserved until a license file is added |

## 2. Contribution process

- **Issue first.** Every change starts from an issue. Bugs and documentation fixes are welcome straight away.
  New features, changes to the vault format, the sync protocol, shortcuts or what the interface shows need a
  maintainer's go-ahead on the issue **before** code is written; pull requests for features without one may be
  closed.
- **Bug reports** answer five questions (as Go asks): the OTPKeep version (Settings → About, or
  `otpkeep version`), the system and CPU, what you did, what you expected and what happened — with demo
  accounts only (§4), never real secrets, links, QR codes, backups or vault files.
- **One pull request, one purpose.** Do not mix a fix, a refactor and a feature; split them.
- **Review:** every change reaches `main` through a pull request with green required checks (§6). Contributors'
  pull requests also need an approval from a code owner (`.github/CODEOWNERS`). Maintainers follow the same rule;
  a documentation-only pull request (no code, tests, workflows or build files) still runs CI's `check` job, which
  sees that nothing testable changed and passes in seconds. Nobody bypasses the checks.
- **Response times** are best effort; security reports follow [SECURITY.md](../../SECURITY.md).

## 3. AI-assisted contributions (MUST)

AI tools may help write code, tests and docs. The person who opens the pull request is its author and is
responsible for every line.

- **Understand it.** You must be able to explain the change, its edge cases and side effects in your own words. If
  you cannot, the pull request is not ready.
- **Verify it.** Run the tests and check the behavior yourself, on the platforms it touches. AI output that "looks
  right" is not evidence.
- **No autonomous agents.** Pull requests and issues come from a person's own account, opened and answered by that
  person. Pull requests that look generated and unattended are closed.
- **Say so** in the pull request description when AI wrote a substantial part of the change, and what it did.
- **Talk in your own words.** Write issues, reviews and replies yourself. Translation help is fine; keep it your
  own voice.
- **Security reports found with AI** must be reproduced by you before you report them, and say that AI was used.
- **Commits carry no tool or bot attribution** (no `Co-Authored-By` for tools, no "generated with" lines): the
  human author is the author. Enforced by the `commit-msg` hook.
- Everything else in these standards applies unchanged, privacy (§4) first: never paste real secrets, accounts,
  backups or private paths into an AI tool's prompt for this project.

## 4. Privacy and test data (MUST)

OTPKeep stores two-factor secrets, so this rule has no exceptions. Enforced by `make privacy` (pre-commit hook and
CI) and review.

- **Never commit real secrets, account names, emails, organization names, hostnames or local paths** — not in
  code, tests, fixtures, docs, screenshots, release notes, commit messages or issues.
- Use only demo data:

  | Kind | Demo values |
  |---|---|
  | Issuers | `Example Cloud`, `Example Mail`, `Example Bank`, `Demo Git`, `示例银行` |
  | Accounts | `alex@example.com`, `demo@example.com`, `ops@example.com`, `demo-user` |
  | Secrets | The test secrets from RFC 4226 / RFC 6238 appendices, `JBSWY3DPEHPK3PXP`, or secrets generated at random inside the test |
  | Device names | `Demo-Mac`, `Demo-PC`, `Demo-Phone` |
  | Hosts | `example.com`, `*.example` ([RFC 2606](https://www.rfc-editor.org/rfc/rfc2606)); LAN addresses from `192.168.0.0/24` |

- **Tests never touch real user data.** All test files live under `/tmp/otpkeep-test/` (`make test` sets
  `TMPDIR`). Tests never read or write the real data directory and never use the real system keystore: they use
  an in-memory keystore (`OTPKEEP_KEYSTORE=memory`) and a data folder under `/tmp` (`OTPKEEP_DATA_DIR`).
- **Pictures:** README screenshots are drawn only by the `readme-shots` browser test from demo data
  (`README_SHOTS=../../docs/images pnpm e2e readme-shots`), so they show nothing of the machine that makes them.
- Each contributor may add their own real identifiers, one regular expression per line, to the git-ignored
  `.privacy-denylist`.

## 5. Security (MUST)

- **Never read other applications' data or keys.** The app reads only its own data directory and its own
  keystore entries. Importing another app's export happens only when the user picks the file or explicitly
  confirms; such files are opened read-only.
- Secrets (two-factor secrets, data keys, device keys, session keys, tokens, pairing and sync codes) are wrapped
  in types that zero their memory on drop (`zeroize`) and never implement a `Debug` or `Display` that shows
  the value.
- Never write secrets or one-time codes to logs, error messages, crash output or analytics. There is no
  telemetry.
- The user interface never receives a secret except through the confirmed export paths (entry QR code, Google
  migration QR codes, plain-text export, pairing QR code). Current codes are shown, never the secrets behind them.
- Use the cryptography crates already chosen for the project. Do not design new primitives or protocols;
  protocol changes need a design review.
- Compare secrets, tokens and confirmation values in constant time.
- Every parser of external input (links, QR payloads, imported files, file headers, network messages) has a
  size limit and a fuzz target, and never panics.
- Write the vault atomically (temporary file, `fsync`, rename) and only while holding the vault lock.
- Network: listen only while a pairing or sync window is open, only on private addresses of physical
  interfaces; the browser management page listens only on `127.0.0.1`, behind a one-time link.
- `unsafe` is allowed only in the Bluetooth platform bindings (`keep-ble`), each block with its reason.

## 6. Supply chain

| Rule | Level | Enforced by |
|---|---|---|
| `main` is protected: changes arrive through pull requests with the `check` job green; no force-push or deletion; linear history; nobody bypasses the checks (§2) | MUST | repository ruleset |
| Release tags `v*` are never moved or deleted once a release is published; nobody can bypass this | MUST | repository ruleset |
| Workflows declare least-privilege `permissions` (read-only by default; `contents: write` only to publish a release) | MUST | review |
| Dependencies are updated by Dependabot: one grouped pull request per ecosystem per week; major versions of crates and npm packages are upgraded on purpose | MUST | `.github/dependabot.yml` |
| A new dependency needs a reason in the pull request: maintenance, size, permissive license (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode; no GPL family) | MUST | `cargo deny`, review |
| `Cargo.lock` and `pnpm-lock.yaml` are committed; CI installs with `--frozen-lockfile` | MUST | CI |
| Known vulnerabilities in dependencies are checked on every change | MUST | `cargo audit` in `make check` |
| Release packages are built in CI only, never on a developer's machine, and published with SHA-256 `checksums.txt`; the install script and Homebrew verify them | MUST | release workflow, `install.sh` |
| Third-party actions are pinned to a full commit SHA (with the version in a comment) | SHOULD | planned |
| Releases carry build provenance (artifact attestations) | SHOULD | planned |

## 7. Code

- **Layout:** Rust crates live in `crates/` with one responsibility each; apps live in `apps/`. Dependencies
  point one way only (apps → services → vault → link parsing → passcode).
- **Size:** functions under 50 lines; files under 400 lines (800 hard limit); nesting at most 4 levels —
  prefer early returns.
- **Pure core:** passcode, parsing, search and merge logic take values and return values. File system,
  keystore, clock, network and platform APIs sit behind traits in `keep-platform`, so the core is testable
  without I/O.
- **Errors:** no `unwrap`, `expect` or `panic!` outside tests (enforced by clippy lints). Library crates
  define error enums with `thiserror`; apps add context. User-facing messages say what went wrong and what
  to do next, and are translated.
- **Immutability:** prefer returning new values over mutating inputs; no global mutable state.
- **No magic values:** name constants for timeouts, limits, ports and file modes; user-tunable defaults are
  defined in one place.
- **Naming:** standard Rust naming; use the project's own domain vocabulary (`Vault`, `Entry`,
  `KeyMaterial`, `Passcode`, `KeyUri`, …).
- **Comments:** doc comments on every public item. Inline comments explain *why*, not *what*.
- **Logging:** `tracing`, off by default beyond warnings; sensitive fields are never logged.
- **Platform code** compiles cleanly on macOS, Linux and Windows: code for one system sits behind `#[cfg(…)]` and
  leaves nothing unused on the others (`clippy -D warnings` in CI on all three).
- **Shell scripts** (`install.sh`, `scripts/`): POSIX `sh` unless bash is needed, `set -eu`, quote every
  expansion, ShellCheck clean (run it in a container before pushing). `install.sh` verifies the checksum before
  installing, uses `sudo` only to install a system package and says so first, and changes nothing but the app,
  its menu entry and one marked `PATH` line.
- **Workflows** pass actionlint (run it in a container before pushing).

## 8. Command line and interface

| Rule | Detail |
|---|---|
| Output streams | Results go to stdout; errors, warnings and progress to stderr |
| Exit codes | Fixed, documented codes (success is `0`, usage error is `64`) |
| Machine output | Commands that report state support `--json`, with a stable schema |
| Secrets | Never accepted as command-line arguments; read from a hidden prompt or stdin |
| Color | Only when stdout is a terminal; respect `NO_COLOR` |
| Prompts | Never prompt when stdin is not a terminal; offer flags instead |
| Destructive or plain-text actions | Preview first and require confirmation; plain-text export needs an explicit flag |
| Languages | Every interface text exists in English and Simplified Chinese, in the same change |
| Language choice | The setting (system, 简体中文, English), shared by every window and the management page; any other system language shows English |
| Shortcuts | Shown the way the platform writes them (`⌃⌥⌘` on macOS, `Ctrl+Alt+Win` elsewhere); tests expect the running platform's form |
| Accessibility | WCAG 2.2 AA: keyboard reachable, visible focus, labels, contrast; light and dark themes |

## 9. Tests (MUST)

- **Test first:** write a failing test, make it pass, then refactor.
- **Coverage:** at least 80% of lines across the workspace (`make cover` fails below it); core crates aim at 90%.
- **Style:** descriptive test names; Arrange–Act–Assert; property tests (`proptest`) for parsers and merge.
- **Vectors:** all RFC 4226 / RFC 6238 test vectors pass.
- **Fuzzing:** every parser has a `cargo-fuzz` target; the full workflow, started by hand before a release, runs each for 10 minutes.
- **Sync simulation:** merge changes pass the deterministic multi-device simulation (`make sim`, 10 000 seeds;
  `make check` runs 300).
- **Isolation:** no network outside loopback and in-memory test networks; no real home directory; no real
  keystore. Tests wait for conditions, never for a fixed time that a slow machine may miss.
- **Never install anything on the developer's machine.** Tests that need other systems or CPUs run in containers
  or in CI: at most one container at a time, with a CPU and memory limit, and none started while the machine is
  at 70% CPU or more.
- **Browser tests** (`pnpm e2e`, Playwright) cover the main window, the search panel, the password window and the
  management page in both themes; they pass on macOS and Linux.
- **Install script:** checked on Ubuntu, Debian, Fedora and Alpine containers (deb, rpm, AppImage, a tampered
  package, musl) and, by the release workflow, on real macOS and Linux x86_64 and arm64 machines that install and
  start every package before it is published.
- **Platforms:** every change passes `make check` and the browser tests; CI runs clippy and the tests on Linux for
  every pull request, and before each release on macOS, Linux and Windows, where it also builds Android and iOS.

## 10. Git workflow

- **Branching:** GitHub Flow. `main` is always releasable; work happens on short-lived branches (`feat/…`,
  `fix/…`, `docs/…`, `refactor/…`, `test/…`, `ci/…`, `chore/…`, `build/…`).
- **Commits:** Conventional Commits, subject at most 72 characters, imperative, enforced by the `commit-msg` hook
  (`make hooks`). One logical change per commit; the body says *why*. Reference issues in the body (`Fixes #123`).
- **Squash** fix-up commits ("address review", "fix typo") before merging; keep separate commits only for
  independent layers of one change. Pull requests are squash-merged with a Conventional Commit title.
- Commits are authored by the contributor's own account, with no tool or bot attribution trailers.
- **Never** force-push to `main` or rewrite published history; a tag whose release is published is never moved
  or deleted.

## 11. Versioning, changelog and releases

- SemVer. Before 1.0, minor versions may contain breaking changes; they are called out in the changelog and in the
  release notes' "Before you upgrade".
- The vault format and the sync protocol have their own version numbers; a change to either needs migration
  tests and an entry in the changelog.
- Every user-visible change adds a line under **Unreleased** in `CHANGELOG.md` (Added / Changed / Deprecated /
  Removed / Fixed / Security), in the user's words. Internal refactors and dependency bumps without user impact
  get no entry; a bump that fixes a vulnerability users were exposed to does.
- **Release notes** are product communication, one file per release in `.github/release-notes/vX.Y.Z.md`, in the
  layout of `.github/release-notes/TEMPLATE.md`. Draft with
  `scripts/release-notes.sh vX.Y.Z > .github/release-notes/vX.Y.Z.md`, then write the TODO parts.
  `CHANGELOG.md` stays the complete engineering record.
- **Release:** move Unreleased entries to a new version section with its date, set the same version in
  `Cargo.toml`, `apps/desktop-mobile/package.json` and `tauri.conf.json`, write the release notes, merge to
  `main`, then tag `vX.Y.Z` on `main` and push the tag (`vX.Y.Z-rc.N` for a pre-release, with its own notes file).
  The release workflow stops when the versions differ or the notes are missing or still have TODOs; then it runs
  every check, builds the universal macOS disk image and the Linux packages (`.deb`, `.rpm`, AppImage on x86_64;
  `.deb`, AppImage on arm64), installs and starts each with `install.sh` on its own machine, publishes them with
  `checksums.txt` and the notes, and updates the Homebrew cask (a pre-release only on the tap's `rc/otpkeep`
  branch).
- **After a release,** verify the release page, a download against `checksums.txt`, the install script and
  `brew install --cask weikxs/tap/otpkeep`.

## 12. Documentation

- Public docs are in English; `README.md` has a Simplified Chinese translation in `README.zh-CN.md` that is
  updated in the same pull request.
- Every command or setting change updates the README and the built-in help text.
- Statements in the docs must be true of the code: a supported platform is built and tested in CI, a promise in
  "Privacy and security" is enforced by a test or a rule above.
- Product requirements, designs and planning are internal and are not published (`docs/prd/` is git-ignored).

## 13. Definition of done

A change is done when:

- [ ] it starts from an issue, and a feature had a maintainer's go-ahead;
- [ ] tests were written first and pass, the browser tests too, on macOS and Linux (CI also runs Windows);
- [ ] `make check` passes (privacy, fmt, clippy, tests, coverage, `cargo deny`, `cargo audit`, performance,
  simulation); scripts are ShellCheck clean and workflows actionlint clean;
- [ ] no real identities, local paths or secrets (`make privacy`);
- [ ] no secret can reach logs, errors or the user interface outside the confirmed export paths;
- [ ] errors are handled and messages are actionable, in both languages;
- [ ] `CHANGELOG.md`, README (both languages) and help text are updated where relevant;
- [ ] the author understands and has verified every line, including AI-assisted parts;
- [ ] the pull request has the `check` job green and is
  squash-merged with a Conventional Commit title.
