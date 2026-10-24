#!/bin/sh
# OTPKeep installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/weikxs/otpkeep/main/install.sh | sh
#
# It picks the package for this system and CPU, downloads it from the GitHub
# release, checks it against the release's checksums.txt and installs it:
#   macOS          OTPKeep.app into /Applications (or ~/Applications)
#   Debian, Ubuntu the .deb, with apt (asks for sudo)
#   Fedora, SUSE   the .rpm, with dnf, zypper or yum (asks for sudo; x86_64)
#   other Linux    the AppImage, as otpkeep-app in ~/.local/bin, with a menu entry
# Nothing else is changed. Your accounts are never touched: they stay in OTPKeep's
# data folder whatever you install, upgrade or remove.
#
# Options (or the environment variables in brackets):
#   --version <tag>     install this release, such as v0.1.0 [OTPKEEP_VERSION]; default: the latest
#   --format <format>   Linux only: deb, rpm or appimage [OTPKEEP_FORMAT]; default: what this system uses
#   --dir <directory>   macOS: where OTPKeep.app goes; AppImage: where otpkeep-app goes [OTPKEEP_INSTALL_DIR]
#   --no-modify-path    AppImage: do not add the directory to your shell's startup file [OTPKEEP_NO_MODIFY_PATH=1]
#   --help              show this help
#
# Run it again to upgrade.
set -eu

REPO="weikxs/otpkeep"
BASE_URL="${OTPKEEP_BASE_URL:-https://github.com/$REPO/releases}"
PATH_MARK="# Added by the OTPKeep installer"
APP_BINARY="otpkeep-app"

say() { printf 'otpkeep-install: %s\n' "$*"; }
fail() {
	printf 'otpkeep-install: error: %s\n' "$*" >&2
	exit 1
}

usage() {
	cat <<'EOF'
OTPKeep installer for macOS and Linux.

  curl -fsSL https://raw.githubusercontent.com/weikxs/otpkeep/main/install.sh | sh
  curl -fsSL https://raw.githubusercontent.com/weikxs/otpkeep/main/install.sh | sh -s -- [options]

Options (or environment variables):
  --version <tag>     install this release, such as v0.1.0 (OTPKEEP_VERSION); default: the latest
  --format <format>   Linux only: deb, rpm or appimage (OTPKEEP_FORMAT); default: what this system uses
  --dir <directory>   macOS: where OTPKeep.app goes; AppImage: where otpkeep-app goes (OTPKEEP_INSTALL_DIR)
  --no-modify-path    AppImage: do not add the directory to your shell's startup file (OTPKEEP_NO_MODIFY_PATH=1)
  --help              show this help

Run it again to upgrade. Your accounts are never touched.
EOF
}

parse_args() {
	version="${OTPKEEP_VERSION:-}"
	format="${OTPKEEP_FORMAT:-}"
	install_dir="${OTPKEEP_INSTALL_DIR:-}"
	modify_path=1
	[ "${OTPKEEP_NO_MODIFY_PATH:-}" = "1" ] && modify_path=0
	while [ $# -gt 0 ]; do
		case "$1" in
		--version) [ $# -ge 2 ] || fail "--version needs a tag, such as v0.1.0"; version="$2"; shift 2 ;;
		--version=*) version="${1#*=}"; shift ;;
		--format) [ $# -ge 2 ] || fail "--format needs deb, rpm or appimage"; format="$2"; shift 2 ;;
		--format=*) format="${1#*=}"; shift ;;
		--dir) [ $# -ge 2 ] || fail "--dir needs a directory"; install_dir="$2"; shift 2 ;;
		--dir=*) install_dir="${1#*=}"; shift ;;
		--no-modify-path) modify_path=0; shift ;;
		-h | --help) usage; exit 0 ;;
		*) fail "unknown option: $1 (see --help)" ;;
		esac
	done
	case "$version" in
	"" | v[0-9]*.[0-9]*.[0-9]*) ;;
	[0-9]*.[0-9]*.[0-9]*) version="v$version" ;;
	*) fail "not a release tag: $version (expected something like v0.1.0)" ;;
	esac
	case "$format" in
	"" | deb | rpm | appimage) ;;
	AppImage) format=appimage ;;
	*) fail "unknown format: $format (expected deb, rpm or appimage)" ;;
	esac
}

detect_platform() {
	case "$(uname -s)" in
	Darwin) os=darwin ;;
	Linux) os=linux ;;
	*) fail "$(uname -s) is not supported yet; OTPKeep runs on macOS and Linux" ;;
	esac
	case "$(uname -m)" in
	x86_64 | amd64) arch=amd64 ;;
	arm64 | aarch64) arch=arm64 ;;
	*) fail "$(uname -m) CPUs are not supported; OTPKeep runs on x86_64 and arm64" ;;
	esac
	[ "$os" = darwin ] && [ -n "$format" ] && fail "--format is for Linux; macOS always gets OTPKeep.app"
	return 0
}

detect_tools() {
	if command -v curl >/dev/null 2>&1; then
		downloader=curl
	elif command -v wget >/dev/null 2>&1; then
		downloader=wget
	else
		fail "curl or wget is needed to download OTPKeep"
	fi
	if command -v sha256sum >/dev/null 2>&1; then
		hasher="sha256sum"
	elif command -v shasum >/dev/null 2>&1; then
		hasher="shasum -a 256"
	else
		fail "sha256sum or shasum is needed to check the download"
	fi
}

# Releases are only fetched over HTTPS; a plain http:// OTPKEEP_BASE_URL (a test server) is the only exception.
secure() {
	case "$BASE_URL" in https://*) return 0 ;; esac
	return 1
}

# download URL FILE
download() {
	if [ "$downloader" = curl ]; then
		if secure; then
			curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"
		else
			curl -fsSL --retry 3 -o "$2" "$1"
		fi
	else
		wget -q -O "$2" "$1"
	fi
}

# latest_version prints the tag of the newest release, read from where
# releases/latest redirects (no API call, so no rate limit).
latest_version() {
	if [ "$downloader" = curl ]; then
		url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$BASE_URL/latest")
	else
		url=$(wget -S --spider "$BASE_URL/latest" 2>&1 | sed -n 's/^ *[Ll]ocation: *//p' | tail -n 1 | tr -d '\r')
	fi
	tag="${url##*/}"
	case "$tag" in
	v[0-9]*.[0-9]*.[0-9]*) printf '%s\n' "$tag" ;;
	*) fail "could not find the latest release at $BASE_URL/latest" ;;
	esac
}

# choose_format picks deb, rpm or appimage for this Linux, unless --format chose one.
choose_format() {
	if ldd --version 2>&1 | grep -qi musl; then
		fail "this Linux uses musl (such as Alpine); OTPKeep needs a glibc-based distribution"
	fi
	if [ -n "$format" ]; then return 0; fi
	if command -v apt-get >/dev/null 2>&1 && command -v dpkg >/dev/null 2>&1; then
		format=deb
	elif command -v rpm >/dev/null 2>&1 &&
		{ command -v dnf >/dev/null 2>&1 || command -v zypper >/dev/null 2>&1 || command -v yum >/dev/null 2>&1; }; then
		if [ "$arch" = amd64 ]; then
			format=rpm
		else
			say "there is no arm64 .rpm; installing the AppImage instead"
			format=appimage
		fi
	else
		format=appimage
	fi
}

asset_name() {
	v="${version#v}"
	if [ "$os" = darwin ]; then
		printf 'otpkeep_%s_darwin_universal.dmg\n' "$v"
		return
	fi
	case "$format" in
	deb) printf 'otpkeep_%s_linux_%s.deb\n' "$v" "$arch" ;;
	rpm) printf 'otpkeep_%s_linux_%s.rpm\n' "$v" "$arch" ;;
	appimage) printf 'otpkeep_%s_linux_%s.AppImage\n' "$v" "$arch" ;;
	esac
}

# fetch_and_verify downloads the package and checksums.txt into $tmp and
# stops unless the package's SHA-256 matches its line in checksums.txt.
fetch_and_verify() {
	file=$(asset_name)
	say "downloading $file ($version)"
	download "$BASE_URL/download/$version/$file" "$tmp/$file" ||
		fail "could not download $file; is $version an OTPKeep release with a $format package for $arch?"
	download "$BASE_URL/download/$version/checksums.txt" "$tmp/checksums.txt" ||
		fail "could not download checksums.txt for $version"
	expected=$(awk -v f="$file" '$2 == f || $2 == "*" f { print $1 }' "$tmp/checksums.txt")
	[ -n "$expected" ] || fail "checksums.txt has no entry for $file"
	actual=$(cd "$tmp" && $hasher "$file" | awk '{ print $1 }')
	[ "$expected" = "$actual" ] || fail "checksum mismatch for $file (expected $expected, got $actual); nothing was installed"
	say "checksum OK"
}

# as_root runs a command as root: directly when already root, otherwise with sudo.
as_root() {
	if [ "$(id -u)" = 0 ]; then
		"$@"
	elif command -v sudo >/dev/null 2>&1; then
		sudo "$@"
	else
		fail "installing a system package needs root; run as root, install sudo, or use --format appimage"
	fi
}

install_deb() {
	command -v apt-get >/dev/null 2>&1 || fail "apt-get is needed for the .deb; use --format appimage instead"
	[ "$(id -u)" = 0 ] || say "installing the .deb with apt; sudo may ask for your password"
	as_root apt-get install -y "$tmp/$file" || fail "apt could not install $file"
}

install_rpm() {
	[ "$(id -u)" = 0 ] || say "installing the .rpm; sudo may ask for your password"
	if command -v dnf >/dev/null 2>&1; then
		as_root dnf install -y "$tmp/$file" || fail "dnf could not install $file"
	elif command -v zypper >/dev/null 2>&1; then
		as_root zypper --non-interactive install --allow-unsigned-rpm "$tmp/$file" || fail "zypper could not install $file"
	elif command -v yum >/dev/null 2>&1; then
		as_root yum install -y "$tmp/$file" || fail "yum could not install $file"
	else
		fail "dnf, zypper or yum is needed for the .rpm; use --format appimage instead"
	fi
}

# expand_dir makes a --dir value absolute.
expand_dir() {
	case "$1" in
	"~"/*) printf '%s\n' "$HOME/${1#"~/"}" ;;
	/*) printf '%s\n' "$1" ;;
	*) printf '%s\n' "$(pwd)/$1" ;;
	esac
}

install_appimage() {
	if [ -z "$install_dir" ]; then
		if [ "$(id -u)" = 0 ]; then install_dir=/usr/local/bin; else install_dir="$HOME/.local/bin"; fi
	fi
	install_dir=$(expand_dir "$install_dir")
	mkdir -p "$install_dir" || fail "cannot create $install_dir"
	[ -w "$install_dir" ] || fail "$install_dir is not writable; choose another with --dir, or run as root"
	chmod 755 "$tmp/$file"
	# Copy next to the target, then rename, so a running OTPKeep is never left half-written.
	cp "$tmp/$file" "$install_dir/.$APP_BINARY.new" || fail "cannot write to $install_dir"
	mv -f "$install_dir/.$APP_BINARY.new" "$install_dir/$APP_BINARY"
	add_menu_entry
	if ! { ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; }; then
		say "note: AppImages need FUSE 2 (the libfuse2 or fuse2 package); without it, start OTPKeep with: $APP_BINARY --appimage-extract-and-run"
	fi
	ensure_path
}

# add_menu_entry puts OTPKeep, with its icon, in the desktop's applications menu.
add_menu_entry() {
	if [ "$(id -u)" = 0 ]; then share=/usr/local/share; else share="${XDG_DATA_HOME:-$HOME/.local/share}"; fi
	icon_line=""
	# The AppImage carries its icon as .DirIcon, usually a link to the picture beside it; unpacking either
	# needs no FUSE.
	dir_icon="$tmp/squashfs-root/.DirIcon"
	if (cd "$tmp" && "./$file" --appimage-extract .DirIcon >/dev/null 2>&1); then
		if [ -L "$dir_icon" ]; then
			target=$(readlink "$dir_icon")
			(cd "$tmp" && "./$file" --appimage-extract "$target" >/dev/null 2>&1) || true
		fi
		if [ -f "$dir_icon" ]; then
			mkdir -p "$share/icons/hicolor/256x256/apps"
			cp -L "$dir_icon" "$share/icons/hicolor/256x256/apps/otpkeep.png" && icon_line="Icon=otpkeep"
		fi
	fi
	mkdir -p "$share/applications"
	{
		printf '[Desktop Entry]\nType=Application\nName=OTPKeep\n'
		printf 'Comment=Two-factor authentication codes\n'
		printf 'Exec=%s\n' "$install_dir/$APP_BINARY"
		[ -n "$icon_line" ] && printf '%s\n' "$icon_line"
		printf 'Categories=Utility;Security;\nTerminal=false\n'
	} >"$share/applications/otpkeep.desktop"
	say "added OTPKeep to the applications menu"
}

on_path() {
	case ":${PATH:-}:" in *":$1:"*) return 0 ;; esac
	return 1
}

# startup_file prints the file the login shell reads for PATH.
startup_file() {
	case "$(basename "${SHELL:-sh}")" in
	zsh) printf '%s\n' "${ZDOTDIR:-$HOME}/.zshrc" ;;
	bash) printf '%s\n' "$HOME/.bashrc" ;;
	fish) printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/otpkeep-path.fish" ;;
	*) printf '%s\n' "$HOME/.profile" ;;
	esac
}

ensure_path() {
	if on_path "$install_dir"; then return 0; fi
	if [ "$modify_path" = 0 ]; then
		say "$install_dir is not on your PATH; add it to start \`$APP_BINARY\` by name"
		return 0
	fi
	rc=$(startup_file)
	if [ -f "$rc" ] && grep -qF "$PATH_MARK" "$rc"; then
		say "$rc already adds the install directory to PATH; open a new terminal"
		return 0
	fi
	mkdir -p "$(dirname "$rc")"
	case "$rc" in
	*.fish) line="fish_add_path -g \"$install_dir\"" ;;
	*) line="export PATH=\"$install_dir:\$PATH\"" ;;
	esac
	printf '\n%s\n%s\n' "$PATH_MARK" "$line" >>"$rc"
	say "added $install_dir to PATH in $rc; open a new terminal (or run: $line)"
}

install_mac() {
	command -v hdiutil >/dev/null 2>&1 || fail "hdiutil is needed to open the disk image"
	if pgrep -x "$APP_BINARY" >/dev/null 2>&1; then
		fail "OTPKeep is running; quit it (menu bar icon → Quit OTPKeep), then run the installer again"
	fi
	if [ -z "$install_dir" ]; then
		if [ -w /Applications ]; then install_dir=/Applications; else install_dir="$HOME/Applications"; fi
	fi
	install_dir=$(expand_dir "$install_dir")
	mkdir -p "$install_dir" || fail "cannot create $install_dir"
	[ -w "$install_dir" ] || fail "$install_dir is not writable; choose another with --dir"
	mount="$tmp/mount"
	mkdir -p "$mount"
	hdiutil attach -nobrowse -readonly -quiet -mountpoint "$mount" "$tmp/$file" || fail "could not open $file"
	mounted=1
	[ -d "$mount/OTPKeep.app" ] || fail "$file has no OTPKeep.app"
	# Copy next to the target, then swap, so an interrupted install never leaves half an app.
	rm -rf "$install_dir/.OTPKeep.app.new"
	ditto "$mount/OTPKeep.app" "$install_dir/.OTPKeep.app.new" || fail "cannot write to $install_dir"
	hdiutil detach -quiet "$mount" && mounted=0
	# Downloaded by this script, not a browser: no "downloaded from the internet" prompt.
	xattr -dr com.apple.quarantine "$install_dir/.OTPKeep.app.new" 2>/dev/null || true
	rm -rf "$install_dir/OTPKeep.app"
	mv "$install_dir/.OTPKeep.app.new" "$install_dir/OTPKeep.app"
	app="$install_dir/OTPKeep.app"
}

# check_installed confirms the app is where it should be.
check_installed() {
	if [ "$os" = darwin ]; then
		[ -x "$app/Contents/MacOS/$APP_BINARY" ] || fail "OTPKeep.app was copied but its program is missing"
		say "installed OTPKeep ${version#v} to $app"
		say "done. Start it from Launchpad or with: open \"$app\"; it lives in the menu bar"
		return
	fi
	case "$format" in
	deb | rpm)
		found=$(command -v "$APP_BINARY" 2>/dev/null || true)
		[ -n "$found" ] || fail "the package was installed but $APP_BINARY is not on PATH"
		say "installed OTPKeep ${version#v} ($format) as $found"
		;;
	appimage)
		[ -x "$install_dir/$APP_BINARY" ] || fail "the AppImage was not installed"
		say "installed OTPKeep ${version#v} (AppImage) as $install_dir/$APP_BINARY"
		;;
	esac
	say "done. Start it from your applications menu or with: $APP_BINARY; it lives in the system tray"
}

cleanup() {
	if [ "${mounted:-0}" = 1 ]; then hdiutil detach -quiet -force "$mount" 2>/dev/null || true; fi
	rm -rf "$tmp"
}

main() {
	parse_args "$@"
	detect_platform
	detect_tools
	[ "$os" = linux ] && choose_format
	[ -n "$version" ] || version=$(latest_version)
	tmp=$(mktemp -d 2>/dev/null || mktemp -d -t otpkeep)
	mounted=0
	trap cleanup EXIT INT TERM
	fetch_and_verify
	if [ "$os" = darwin ]; then
		install_mac
	else
		case "$format" in
		deb) install_deb ;;
		rpm) install_rpm ;;
		appimage) install_appimage ;;
		esac
	fi
	check_installed
}

main "$@"
