#!/bin/sh
# Drafts the release notes of one version from its section of CHANGELOG.md,
# in the layout of .github/release-notes/TEMPLATE.md:
#
#   scripts/release-notes.sh v0.2.0 > .github/release-notes/v0.2.0.md
#
# The draft fills in what the changelog knows (date, summary, upgrading
# steps, new features, changes, fixes, security, downloads, links) and
# leaves TODO comments where a person writes: the theme, who the release is
# for and what you can do now. Sections with nothing to say are left out.
# The release workflow publishes the edited file, not this draft. A
# pre-release (v0.2.0-rc.1) is drafted from its version's section (0.2.0).
# Fails when the changelog has no section for the version.
#
# usage: scripts/release-notes.sh <tag> [CHANGELOG.md]
set -eu

tag=${1:?usage: release-notes.sh <tag> [changelog]}
changelog=${2:-CHANGELOG.md}
version=${tag#v}
repo=https://github.com/weikxs/otpkeep

# trim drops leading and trailing blank lines.
trim() { sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'; }

# A pre-release without a section of its own uses its version's section.
if ! grep -q "^## \[$version\]" "$changelog"; then version=${version%%-*}; fi

# section prints the lines under "### $1" in this version's section.
section() {
	awk -v version="$version" -v want="$1" '
		/^## \[/ { inside = index($0, "## [" version "]") == 1; next }
		!inside { next }
		/^### / { current = substr($0, 5); next }
		current == want { print }
	' "$changelog" | trim
}

# block prints a heading and the CHANGELOG category, unless it is empty.
block() {
	body=$(section "$2")
	if [ -n "$body" ]; then printf '%s\n%s\n\n' "$1" "$body"; fi
}

heading=$(grep -m1 "^## \[$version\]" "$changelog" || true)
if [ -z "$heading" ]; then
	echo "release-notes: CHANGELOG has no section for $version" >&2
	exit 1
fi
date=${heading##* - }
case "$date" in [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]) ;; *) date=TODO ;; esac
summary=$(awk -v version="$version" '
	/^## \[/ { inside = index($0, "## [" version "]") == 1; next }
	inside && /^### / { exit }
	inside { print }
' "$changelog" | trim)
# The version released before this one: the next section below it.
previous=$(awk -v version="$version" '
	/^## \[/ { if (found) { sub(/^## \[/, ""); sub(/\].*/, ""); print; exit } if (index($0, "## [" version "]") == 1) found = 1 }
' "$changelog")
files=${tag#v}
upgrading=$(section Upgrading)
removed=$(section Removed)
deprecated=$(section Deprecated)

cat <<EOF
# OTPKeep $tag — <!-- TODO: one-line theme -->

Released on \`$date\`.

> OTPKeep is a free, private authenticator for TOTP, HOTP and Steam codes that lives in your menu bar: a click, a
> shortcut or a few typed letters give you the code, and your accounts never leave your own devices.

## Summary

${summary:-<!-- TODO: two or three sentences: what this release is about and what changes for you. -->}

## Who this release is for

<!-- TODO: the person this release helps most, specifically. -->

EOF
if [ -n "$upgrading$removed$deprecated" ]; then
	printf '## Before you upgrade\n\n'
	if [ -n "$upgrading" ]; then printf '%s\n\n' "$upgrading"; fi
	block "### Removed" Removed
	block "### Deprecated" Deprecated
fi
cat <<EOF
## What you can do now

<!-- TODO: 3–6 short paragraphs, one per capability, from the reader's side. -->

EOF
if [ -n "$(section Added)$(section Changed)" ]; then
	printf "## What's new\n\n"
	block "### New" Added
	block "### Changed" Changed
fi
block "## Bug fixes" Fixed
cat <<EOF
## Privacy and security

EOF
security=$(section Security)
if [ -n "$security" ]; then printf '%s\n' "$security"; fi
cat <<EOF
- **Encrypted on your device.** Accounts are stored with XChaCha20-Poly1305; the key lives in the system keychain, or
  is derived from an optional master password with Argon2id.
- **No account, no server, no telemetry.** OTPKeep uses the network only for sync with your own devices nearby while
  a window is open, and for the management page on \`127.0.0.1\`.
- **Sync you can trust.** The Noise protocol between your devices' Ed25519 identities; the one-time code is checked
  with SPAKE2, so a recorded session cannot be guessed offline.
- **Careful with the clipboard.** Copied codes are kept out of clipboard history apps and cleared after 30 seconds
  (adjustable), only if the clipboard still holds that code.
- **Never reads what is not its own.** OTPKeep never opens keys, passwords or keychain items other than its own.

## Install

- Script (macOS and Linux, fully automatic): \`curl -fsSL https://raw.githubusercontent.com/weikxs/otpkeep/main/install.sh | sh\`
- Homebrew (macOS): \`brew install --cask weikxs/tap/otpkeep\`, or \`brew upgrade --cask otpkeep\`
- A package below; check it against \`checksums.txt\`

| File | Platform |
|---|---|
| \`otpkeep_${files}_darwin_universal.dmg\` | macOS, Apple silicon and Intel |
| \`otpkeep_${files}_linux_amd64.deb\` / \`_arm64.deb\` | Debian, Ubuntu, Raspberry Pi OS (x86_64 / arm64) |
| \`otpkeep_${files}_linux_amd64.rpm\` | Fedora, RHEL, openSUSE (x86_64) |
| \`otpkeep_${files}_linux_amd64.AppImage\` / \`_arm64.AppImage\` | Any Linux with glibc (x86_64 / arm64) |

Then open OTPKeep — it appears in the menu bar (macOS) or the system tray (Linux) — and add your first account.

## Compatibility

- macOS 11 (Big Sur) or newer on Apple silicon and Intel.
- Linux with glibc on x86_64 and arm64: Ubuntu 22.04+, Debian 12+, Fedora 38+ and their relatives; other
  distributions through the AppImage. Typing codes works on X11, not on Wayland.
- English and Simplified Chinese; other system languages show English.
- Windows, Android and iOS are planned.

## Notes

- The macOS app is not signed by Apple yet; Homebrew and the install script remove the quarantine flag. See
  [SECURITY.md]($repo/blob/$tag/SECURITY.md).
EOF
if [ -n "$previous" ]; then
	printf -- '- **Full changelog:** [v%s...%s](%s/compare/v%s...%s) · [CHANGELOG.md](%s/blob/%s/CHANGELOG.md)\n' \
		"$previous" "$tag" "$repo" "$previous" "$tag" "$repo" "$tag"
else
	printf -- '- **Full changelog:** [CHANGELOG.md](%s/blob/%s/CHANGELOG.md)\n' "$repo" "$tag"
fi
cat <<EOF
- Documentation: [README]($repo#readme) · [简体中文]($repo/blob/main/README.zh-CN.md)
- Feedback and problems: [issues]($repo/issues).
EOF
