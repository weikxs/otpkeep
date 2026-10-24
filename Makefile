# Common tasks. `make check` is what CI runs; run it before every pull request.

UI_DIR := apps/desktop-mobile
# Tests write only under /tmp (docs/development/standards.md, section 2).
TEST_TMP := /tmp/otpkeep-test
export TMPDIR := $(TEST_TMP)/

# Coverage floor for the Rust workspace; core crates aim higher as they fill in (90%).
COVER_MIN := 80
# The app crate is the Tauri shell; its logic lives in the core crates.
COVER_EXCLUDE := --exclude otpkeep-app

.PHONY: all build ui ui-install test test-rust test-ui cover fmt fmt-check clippy deny audit privacy sim lan-smoke \
        check perf fuzz e2e hooks app clean

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
	$(MAKE) sim SIM_SEEDS=300

# Performance gates, measured on release builds (PRD FR-3.10: search within 16 ms for 1000 entries).
perf: $(TEST_TMP)
	cargo test --release --workspace --test performance -- --nocapture

# Sync simulation (design/09-testing.md §2.6) on a release build; the nightly run uses 10 000 seeds.
# Replay one seed with: OTPKEEP_SIM_SEED=<seed> cargo test --release -p keep-sync --test simulation
SIM_SEEDS ?= 10000
sim: $(TEST_TMP)
	OTPKEEP_SIM_SEEDS=$(SIM_SEEDS) cargo test --release -p keep-sync --test simulation

# Pairs and syncs two command-line processes on this machine over the real local network (needs a usable
# Wi-Fi or Ethernet connection; uses temporary data and a master password, never the keychain or real vault).
lan-smoke: $(TEST_TMP)
	cargo build -p otpkeep-cli
	scripts/lan-smoke.py target/debug/otpkeep

# Fuzz every parser of outside data (needs nightly Rust and cargo-fuzz). FUZZ_SECONDS per target.
FUZZ_SECONDS ?= 60
FUZZ_TARGETS := keyuri base32 migration vault_doc sealed goldenpassport qr_image find
fuzz:
	cd fuzz && for target in $(FUZZ_TARGETS); do \
		cargo +nightly fuzz run $$target -- -max_total_time=$(FUZZ_SECONDS) -rss_limit_mb=2048 || exit 1; \
	done

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
