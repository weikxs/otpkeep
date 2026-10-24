#!/bin/sh
# Copies what `tauri build` produced into a folder under the release's file names:
#   otpkeep_<version>_darwin_universal.dmg
#   otpkeep_<version>_linux_<arch>.deb | .rpm | .AppImage
# Fails when an expected package is missing.
#
# usage: scripts/release/rename-bundles.sh <version> darwin|linux <amd64|arm64|universal> <out> <expected kinds...>
#   e.g. rename-bundles.sh 0.1.0 linux amd64 dist deb rpm appimage
set -eu

version=${1:?usage: rename-bundles.sh <version> <os> <arch> <out> <kinds...>}
os=${2:?os}
arch=${3:?arch}
out=${4:?out}
shift 4
[ $# -gt 0 ] || {
	echo "rename-bundles: name at least one kind (dmg, deb, rpm, appimage)" >&2
	exit 1
}
mkdir -p "$out"

# one finds exactly one file under target/**/bundle/<dir> with this extension.
one() {
	found=$(find target -path "*/bundle/$1/*" -name "*.$2" -type f 2>/dev/null)
	count=$(printf '%s\n' "$found" | grep -c . || true)
	[ "$count" = 1 ] || {
		echo "rename-bundles: expected one .$2 under target/**/bundle/$1, found $count" >&2
		[ -z "$found" ] || printf '%s\n' "$found" >&2
		exit 1
	}
	printf '%s\n' "$found"
}

for kind in "$@"; do
	case "$kind" in
	dmg) src=$(one dmg dmg) ext=dmg ;;
	deb) src=$(one deb deb) ext=deb ;;
	rpm) src=$(one rpm rpm) ext=rpm ;;
	appimage) src=$(one appimage AppImage) ext=AppImage ;;
	*)
		echo "rename-bundles: unknown kind $kind" >&2
		exit 1
		;;
	esac
	name="otpkeep_${version}_${os}_${arch}.$ext"
	cp "$src" "$out/$name"
	echo "rename-bundles: $src -> $out/$name"
done
