# Contributing to OTPKeep

Thanks for helping! This page is the short version; the full rules are in
[docs/development/standards.md](docs/development/standards.md).

> The project has not chosen a license yet. Until a license file is added, all rights are reserved.
> Please open an issue to discuss before starting larger work.

## Set up

Requirements: Rust (the version in `rust-toolchain.toml` is installed automatically by rustup), Node.js 22,
pnpm 11, Make, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.
Optional tools: `cargo install --locked cargo-deny cargo-audit cargo-llvm-cov`.

```sh
git clone https://github.com/weikxs/otpkeep.git
cd otpkeep
make hooks     # install the pre-commit and commit-msg hooks
make check     # privacy, format, clippy, tests, coverage, dependency policy, advisories
make e2e       # browser tests of the shared UI (needs Chrome)
make app       # debug app bundle for your platform
```

Mobile builds: `pnpm tauri ios build --debug --target aarch64-sim` and
`pnpm tauri android build --debug --apk` in `apps/desktop-mobile` (Xcode, or the Android SDK and NDK).

## Workflow

1. Open or pick an issue, and say you are working on it.
2. Create a branch from `main`: `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, …
3. Write a failing test first, then the code that makes it pass.
4. Keep the change focused. Update `CHANGELOG.md` under **Unreleased** for anything users notice.
5. Run `make check`, then open a pull request using the template.

## Commit messages

[Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/), enforced by the `commit-msg` hook:

```text
feat(passcode): support SHA-512 secrets
fix(app): keep the quick panel inside the active display
```

Commits are authored by you only; do not add tool or bot attribution trailers.

## Test data

Use demo data only: issuers such as `Example Cloud`, accounts such as `alex@example.com`, and the RFC 4226 /
RFC 6238 test secrets. **Never put a real two-factor secret, otpauth link, QR code, backup or vault file in
the repository, an issue or a pull request.** `make privacy` blocks the common cases.

## Security

Report vulnerabilities privately; see [SECURITY.md](SECURITY.md).
