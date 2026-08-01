#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
mode="${1:-release}"

case "$mode" in
  debug|release) ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 64 ;;
esac

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: required tool '$1' was not found" >&2
    exit 69
  }
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS app packaging requires macOS" >&2
  exit 69
fi
require_command cargo
require_command xcrun
require_command sed
xcrun --find metal >/dev/null 2>&1 || {
  echo "error: Metal Toolchain is missing. Install it with: xcodebuild -downloadComponent MetalToolchain" >&2
  exit 69
}
xcrun --find swift >/dev/null 2>&1 || {
  echo "error: Swift toolchain is missing (xcrun --find swift)" >&2
  exit 69
}
xcrun --find iconutil >/dev/null 2>&1 || {
  echo "error: iconutil is missing (install Xcode Command Line Tools)" >&2
  exit 69
}

version="$(sed -n 's/^[[:space:]]*version[[:space:]]*= *"\([^"]*\)".*/\1/p' "$repo_root/Cargo.toml" | head -n 1)"
[[ -n "$version" ]] || { echo "error: could not read workspace version" >&2; exit 65; }

build_dir="$repo_root/target/$mode"
dist_dir="$repo_root/target/dist"
packaging_dir="$repo_root/target/packaging"
app="$dist_dir/Spotuify.app"
rm -rf "$app" "$packaging_dir"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$packaging_dir"

source_iconset="$repo_root/clients/macos/Sources/Spotuify/Assets.xcassets/AppIcon.appiconset"
xcrun swift "$repo_root/packaging/make-icon.swift" "$source_iconset" "$packaging_dir/AppIcon.iconset"
xcrun iconutil -c icns "$packaging_dir/AppIcon.iconset" -o "$app/Contents/Resources/AppIcon.icns"

cp "$build_dir/spotuify-desktop" "$app/Contents/MacOS/SpotuifyDesktop"
cp "$build_dir/spotuify" "$app/Contents/Resources/spotuify"
chmod 755 "$app/Contents/MacOS/SpotuifyDesktop" "$app/Contents/Resources/spotuify"
sed "s/@VERSION@/$version/g" "$repo_root/packaging/Info.plist" > "$app/Contents/Info.plist"

echo "Built: $app"
