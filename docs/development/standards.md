# Development Standards

These rules apply to every change. "MUST" rules are enforced by tooling or review; "SHOULD" rules need a
reason in the pull request when broken.

## 1. Standards we follow

| Area | Standard |
|---|---|
| Versioning | [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) |
| Commit messages | [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) |
| Changelog | [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) |
| Rust style | [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/), `rustfmt`, `clippy` with `-D warnings` |
| TypeScript style | `strict` mode, ESLint, Prettier |
| One-time passwords | [RFC 4226](https://www.rfc-editor.org/rfc/rfc4226) (HOTP), [RFC 6238](https://www.rfc-editor.org/rfc/rfc6238) (TOTP), [RFC 4648](https://www.rfc-editor.org/rfc/rfc4648) (Base32) |
| Community | [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) |
| Security reporting | Private GitHub Security Advisories ([SECURITY.md](../../SECURITY.md)) |
| User interface | [WCAG 2.2](https://www.w3.org/TR/WCAG22/) level AA |
| CLI behavior | [Command Line Interface Guidelines](https://clig.dev/) |
| License | Not chosen yet; all rights reserved until a license file is added |

## 2. Privacy and test data (MUST)

OTPKeep stores two-factor secrets, so this rule has no exceptions.

- **Never commit real secrets, account names, emails, organization names, hostnames or local paths** — not in
  code, tests, fixtures, docs, screenshots, commit messages or issues.
- Use only demo data:

  | Kind | Demo values |
  |---|---|
  | Issuers | `Example Cloud`, `Demo Git`, `示例银行` |
  | Accounts | `alex@example.com`, `alex-work@example.org`, `demo-user` |
  | Secrets | The test secrets from RFC 4226 / RFC 6238 appendices, or secrets generated at random inside the test |
  | LAN names | `Demo-Mac`, `Demo-PC`, `Demo-Phone` |
  | Hosts | `example.com`, `*.example` ([RFC 2606](https://www.rfc-editor.org/rfc/rfc2606)); LAN addresses from `192.168.0.0/24` |

- **Tests never touch real user data.** All test files live under `/tmp/otpkeep-test/` (set `TMPDIR` in
  `make test`). Tests never read or write the real data directory, and never use the real system keystore:
  they use an in-memory keystore or a test-only service name that is removed afterwards.
- `make privacy` (pre-commit hook and CI) blocks local home paths, private keys, tokens and `otpauth://` links
  with secrets that are not demo secrets. Each contributor may add their own real identifiers, one regular
  expression per line, to the git-ignored `.privacy-denylist`.

## 3. Security (MUST)

- **Never read other applications' data or keys.** The app reads only its own data directory and its own
  keystore entries. Importing another app's export happens only when the user picks the file or explicitly
  confirms; such files are opened read-only.
- Secrets (two-factor secrets, data keys, device keys, session keys, tokens, pairing and sync codes) are wrapped
  in types that zero their memory on drop (`zeroize`) and never implement a `Debug` or `Display` that shows
  the value.
- Never write secrets or one-time codes to logs, error messages, crash output or analytics. There is no
  telemetry.
- The user interface never receives a secret except through the four confirmed export paths
  (entry QR code, Google migration QR codes, plain-text export, pairing QR code).
- Use the cryptography crates already chosen for the project. Do not design new primitives or protocols;
  protocol changes need a design review.
- Compare secrets, tokens and confirmation values in constant time.
- Every parser of external input (links, QR payloads, imported files, file headers, network messages) has a
  size limit and a fuzz target, and never panics.
- Write the vault atomically (temporary file, `fsync`, rename) and only while holding the vault lock.
- Network: listen only while a pairing or sync window is open, only on private addresses of physical
  interfaces; local web pages and the local HTTP API listen only on loopback.

## 4. Code

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
- **Unsafe:** `#![forbid(unsafe_code)]` in core crates; platform glue that needs `unsafe` documents why.
- **Dependencies:** a new dependency needs a reason in the pull request (maintenance, size, license).
  Only permissive licenses (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode); no GPL-family licenses
  (`cargo deny` enforces this).

## 5. Command-line behavior

| Rule | Detail |
|---|---|
| Output streams | Results go to stdout; errors, warnings and progress to stderr |
| Exit codes | Fixed, documented codes (success is `0`, usage error is `64`) |
| Machine output | Commands that report state support `--json`, with a stable schema |
| Secrets | Never accepted as command-line arguments; read from a hidden prompt or stdin |
| Color | Only when stdout is a terminal; respect `NO_COLOR` |
| Prompts | Never prompt when stdin is not a terminal; offer flags instead |
| Destructive or plain-text actions | Preview first and require confirmation; plain-text export needs an explicit flag |

## 6. Tests (MUST)

- **Test first:** write a failing test, make it pass, then refactor.
- **Coverage:** core crates at least 90%, overall at least 80% (`make cover` fails below it).
- **Style:** descriptive test names; Arrange–Act–Assert; property tests (`proptest`) for parsers and merge.
- **Vectors:** all RFC 4226 / RFC 6238 test vectors must pass.
- **Fuzzing:** every parser has a `cargo-fuzz` target; nightly CI runs each for 10 minutes.
- **Sync simulation:** merge changes must pass the deterministic multi-device simulation (10 000 seeds).
- **Isolation:** no network outside loopback and in-memory test networks; no real home directory; no real
  keystore.
- **End-to-end:** WebDriver on Windows and Linux, Playwright for shared UI components and the local
  browser page, emulator smoke tests for iOS and Android. Platform behavior that cannot be automated is on
  the manual checklist and signed off before each release.
- Tests must pass on macOS, Windows and Linux.

## 7. Git workflow

- **Branching:** GitHub Flow. `main` is always releasable; work happens on short-lived branches
  (`feat/…`, `fix/…`, `docs/…`, `refactor/…`, `test/…`, `ci/…`, `chore/…`).
- **Commits:** Conventional Commits, enforced by the `commit-msg` hook. One logical change per commit.
  Commits are authored only by the contributor's own account; do not add tool or bot attribution trailers.
- **Pull requests:** use the template, link the issue, keep them small. CI must be green; at least one
  approval from a code owner; squash-merge with a Conventional Commit title.
- **Never** force-push to `main` or rewrite published history.

## 8. Versioning and releases

- SemVer. Before 1.0, minor versions may contain breaking changes; they are called out in the changelog.
- The vault format and the sync protocol have their own version numbers; a change to either needs
  migration tests and an entry in the changelog.
- Every user-visible change adds a line under **Unreleased** in `CHANGELOG.md`
  (Added / Changed / Deprecated / Removed / Fixed / Security).
- Release: move Unreleased entries to a new version section, tag `vX.Y.Z` on `main`, push the tag. The
  release workflow runs the checks, builds all five platforms, signs and notarizes where possible, and
  publishes checksums.

## 9. Documentation

- Public docs are in English; `README.md` has a Simplified Chinese translation in `README.zh-CN.md` that is
  kept in sync in the same pull request.
- Every command or setting change updates the README and the built-in help text.
- Product requirements, designs and planning are internal and are not published (`docs/prd/` is
  git-ignored).

## 10. Definition of done

A change is done when:

- [ ] tests were written first and pass on macOS, Windows and Linux;
- [ ] `make check` passes (fmt, clippy, tests, coverage, `cargo deny`, `cargo audit`, privacy);
- [ ] no real identities, local paths or secrets (`make privacy`);
- [ ] no secret can reach logs, errors or the user interface outside the confirmed export paths;
- [ ] errors are handled and messages are actionable;
- [ ] `CHANGELOG.md`, README and help text are updated where relevant;
- [ ] the pull request is reviewed and merged with a Conventional Commit title.
