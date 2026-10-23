# Common tasks. `make check` is what CI runs; run it before every pull request.

UI_DIR := apps/desktop-mobile
# Tests write only under /tmp (docs/development/standards.md, section 2).
TEST_TMP := /tmp/otpkeep-test
export TMPDIR := $(TEST_TMP)/

# Coverage floor for the Rust workspace; core crates aim higher as they fill in (90%).
COVER_MIN := 80
# The app crate is the Tauri shell; its logic lives in the core crates.
COVER_EXCLUDE := --exclude otpkeep-app

.PHONY: all build ui ui-install test test-rust test-ui cover fmt fmt-check clippy deny audit privacy \
        check perf e2e hooks app clean

all: check

$(TEST_TMP):
	@mkdir -p $(TEST_TMP)

ui-install:
	cd $(UI_DIR) && pnpm install --frozen-lockfile

# The Tauri crate embeds the built UI, so Rust builds of the app need it first.
ui: ui-install
	cd $(UI_DIR) && pnpm build

build: ui
	cargo build --workspace

test: test-rust test-ui

test-rust: ui $(TEST_TMP)
	cargo test --workspace

test-ui: ui-install $(TEST_TMP)
	cd $(UI_DIR) && pnpm typecheck && pnpm test

cover: ui $(TEST_TMP)
	cargo llvm-cov --workspace $(COVER_EXCLUDE) --fail-under-lines $(COVER_MIN)

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

clippy: ui
	cargo clippy --workspace --all-targets -- -D warnings

deny:
	cargo deny check

audit:
	cargo audit --ignore RUSTSEC-2024-0370

privacy:
	scripts/check-privacy.sh

check: privacy fmt-check clippy test cover deny audit perf

# Performance gates, measured on release builds (PRD FR-3.10: search within 16 ms for 1000 entries).
perf: $(TEST_TMP)
	cargo test --release --workspace --test performance -- --nocapture

# Browser tests of the shared UI against a production build (uses the local Chrome).
e2e: ui-install
	cd $(UI_DIR) && pnpm e2e

# Desktop app bundle for the current platform (debug).
app: ui-install
	cd $(UI_DIR) && pnpm tauri build --debug

hooks:
	git config core.hooksPath scripts/hooks
	@echo "git hooks installed (scripts/hooks)"

clean:
	cargo clean
	rm -rf $(UI_DIR)/dist $(UI_DIR)/node_modules $(UI_DIR)/test-results $(TEST_TMP)
