#!/bin/sh
# Prints the Homebrew cask for a release from packaging/homebrew/otpkeep.rb.tmpl, with the
# version and the SHA-256 of its universal disk image.
#
# usage: scripts/release/homebrew-cask.sh <version> <path to otpkeep_<version>_darwin_universal.dmg>
set -eu

version=${1:?usage: homebrew-cask.sh <version> <dmg>}
dmg=${2:?usage: homebrew-cask.sh <version> <dmg>}
[ -f "$dmg" ] || {
	echo "homebrew-cask: $dmg not found" >&2
	exit 1
}
case "$(basename "$dmg")" in
"otpkeep_${version}_darwin_universal.dmg") ;;
*)
	echo "homebrew-cask: $dmg is not otpkeep_${version}_darwin_universal.dmg" >&2
	exit 1
	;;
esac
if command -v sha256sum >/dev/null 2>&1; then
	sha=$(sha256sum "$dmg" | awk '{ print $1 }')
else
	sha=$(shasum -a 256 "$dmg" | awk '{ print $1 }')
fi
sed -e "s/@VERSION@/$version/" -e "s/@SHA256@/$sha/" packaging/homebrew/otpkeep.rb.tmpl
