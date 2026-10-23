#!/usr/bin/env bash
# Fails when tracked or about-to-be-tracked files contain local paths, secrets,
# real two-factor secrets, or any identifier listed in the git-ignored
# .privacy-denylist file. Docs, examples and tests use demo data only
# (see docs/development/standards.md, section 2).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

readonly DENYLIST=".privacy-denylist"
readonly SELF="scripts/check-privacy.sh"

# Real home directories. Tests use /tmp/otpkeep-test instead.
home_patterns=(
  '(^|[^A-Za-z0-9._/-])/Users/[A-Za-z0-9._-]+/'    # macOS home directories
  '(^|[^A-Za-z0-9._/-])/home/[a-z_][a-z0-9._-]*/'  # Linux home directories
  '[A-Za-z]:\\Users\\'                              # Windows home directories
)

# Generic patterns that are never acceptable in this repository.
patterns=(
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'             # private keys
  'gh[pousr]_[A-Za-z0-9]{36}'                       # GitHub tokens
  'github_pat_[A-Za-z0-9_]{22,}'                    # GitHub fine-grained tokens
  'AKIA[0-9A-Z]{16}'                                # AWS access keys
  'otpauth-migration://offline\?data=[A-Za-z0-9%+/=]{40,}' # Google migration payloads (generate them in tests)
)

# Two-factor secrets in otpauth links must be demo secrets: the RFC 4226 / 6238 test
# secrets (Base32 of "1234567890...") or the widely used example JBSWY3DPEHPK3PXP.
readonly SECRET_PARAM='secret=[A-Za-z2-7=]{8,}'
readonly DEMO_SECRET='secret=(GEZDGNBVGY3TQOJQ[A-Z2-7]*=*|JBSWY3DPEHPK3PXP)([^A-Za-z2-7=]|$)'

# Real identifiers (names, accounts, companies) live only in the local denylist.
if [[ -f "$DENYLIST" ]]; then
  while IFS= read -r line; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    patterns+=("$line")
  done <"$DENYLIST"
fi

# search prints file:line:text for every match. --untracked covers new files;
# ignored files (docs/prd, the denylist, build output) are skipped.
search() {
  local flags=$1
  shift
  local args=()
  for p in "$@"; do
    args+=(-e "$p")
  done
  git grep --untracked -n -I $flags -E "${args[@]}" -- . ":(exclude)$SELF" ':(exclude)*.lock' ':(exclude)pnpm-lock.yaml' 2>/dev/null || true
}

real_secrets=$(search "" "$SECRET_PARAM" | grep -Ev "$DEMO_SECRET" || true)

matches=$(
  search -i "${patterns[@]}"
  search -i "${home_patterns[@]}"
  printf '%s\n' "$real_secrets"
)
matches=$(sed '/^$/d' <<<"$matches")

if [[ -n "$matches" ]]; then
  echo "privacy check failed: remove local paths, secrets or real identifiers from:" >&2
  # Print locations only, never the matched text, so secrets are not echoed.
  cut -d: -f1,2 <<<"$matches" | sort -u >&2
  exit 1
fi

echo "privacy check passed"
