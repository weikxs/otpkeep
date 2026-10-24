#!/bin/sh
# Installs freshly built packages with the real install.sh, from a local stand-in for the GitHub release,
# then starts the installed app and checks it keeps running. Used by the release workflow on each build
# machine before anything is published. The app runs on a throwaway data folder with an in-memory keystore.
#
# usage: scripts/release/verify-install.sh <tag> <folder with the release files> macos|deb|appimage
set -eu

tag=${1:?usage: verify-install.sh <tag> <dist> <mode>}
dist=${2:?dist}
mode=${3:?mode}
version=${tag#v}
RUN_SECONDS=15
PORT=8765

work=$(mktemp -d)
server=""
cleanup() {
	[ -z "$server" ] || kill "$server" 2>/dev/null || true
	rm -rf "$work"
}
trap cleanup EXIT INT TERM
say() { echo "verify-install: $*"; }
fail() {
	echo "verify-install: FAILED: $*" >&2
	exit 1
}

# The release as install.sh sees it: <base>/download/<tag>/<file> and checksums.txt.
mkdir -p "$work/release/download/$tag"
cp "$dist"/otpkeep_* "$work/release/download/$tag/"
(cd "$work/release/download/$tag" && { sha256sum otpkeep_* 2>/dev/null || shasum -a 256 otpkeep_*; } >checksums.txt)
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$work/release" >/dev/null 2>&1 &
server=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
	curl -fs -o /dev/null "http://127.0.0.1:$PORT/download/$tag/checksums.txt" && break
	sleep 1
done
export OTPKEEP_BASE_URL="http://127.0.0.1:$PORT"

# runs keeps the app running for RUN_SECONDS; it must not exit before.
runs() {
	export OTPKEEP_DATA_DIR="$work/data" OTPKEEP_KEYSTORE=memory
	mkdir -p "$OTPKEEP_DATA_DIR"
	"$@" >"$work/app.log" 2>&1 &
	app=$!
	sleep "$RUN_SECONDS"
	if kill -0 "$app" 2>/dev/null; then
		kill "$app" 2>/dev/null || true
		say "the app started and kept running for ${RUN_SECONDS}s"
	else
		cat "$work/app.log" >&2
		fail "the app exited within ${RUN_SECONDS}s"
	fi
}

case "$mode" in
macos)
	apps="$work/Applications"
	sh ./install.sh --version "$tag" --dir "$apps"
	app="$apps/OTPKeep.app"
	archs=$(lipo -archs "$app/Contents/MacOS/otpkeep-app")
	for want in x86_64 arm64; do
		case " $archs " in *" $want "*) ;; *) fail "not universal: $archs (no $want)" ;; esac
	done
	minos=$(/usr/libexec/PlistBuddy -c "Print :LSMinimumSystemVersion" "$app/Contents/Info.plist")
	[ "$minos" = 11.0 ] || fail "LSMinimumSystemVersion is $minos, expected 11.0"
	[ -z "$(xattr -p com.apple.quarantine "$app" 2>/dev/null || true)" ] || fail "the quarantine flag is still set"
	say "OTPKeep.app is universal ($archs), needs macOS $minos, no quarantine flag"
	runs "$app/Contents/MacOS/otpkeep-app"
	;;
deb)
	sh ./install.sh --version "$tag" --format deb
	dpkg -s otp-keep | grep -q "^Version: ${version%%-*}" || fail "dpkg does not list otp-keep ${version%%-*}"
	missing=$(ldd "$(command -v otpkeep-app)" | grep "not found" || true)
	[ -z "$missing" ] || fail "missing libraries: $missing"
	runs dbus-run-session -- xvfb-run -a otpkeep-app
	;;
appimage)
	sh ./install.sh --version "$tag" --format appimage --no-modify-path
	# Where install.sh puts it: for root /usr/local, otherwise the user's own folders.
	if [ "$(id -u)" = 0 ]; then
		bin=/usr/local/bin/otpkeep-app share=/usr/local/share
	else
		bin="$HOME/.local/bin/otpkeep-app" share="${XDG_DATA_HOME:-$HOME/.local/share}"
	fi
	[ -x "$bin" ] || fail "$bin was not installed"
	grep -q "^Icon=otpkeep$" "$share/applications/otpkeep.desktop" || fail "the menu entry has no icon"
	runs dbus-run-session -- xvfb-run -a "$bin" --appimage-extract-and-run
	;;
*) fail "unknown mode $mode (macos, deb or appimage)" ;;
esac
say "$mode OK for $tag"
