#!/usr/bin/env bash
set -euo pipefail

# Builds the Release Spotuify.app and packages it into a distributable DMG.
#
# The DMG is UNSIGNED: there is no Apple Developer ID identity and no
# notarization. On first launch users must right-click the app and choose
# Open (Gatekeeper will otherwise refuse an unsigned/quarantined bundle).
#
# Usage:
#   scripts/build-dmg.sh [VERSION]
#   SPOTUIFY_VERSION=0.1.44 scripts/build-dmg.sh
#
# Version resolution order: $1 arg, then $SPOTUIFY_VERSION, then the workspace
# version in the repo-root Cargo.toml.
#
# Output: clients/macos/dist/Spotuify-<version>.dmg

# --- locate ourselves ---------------------------------------------------------
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
macos_dir="$(cd "$script_dir/.." && pwd)"
repo_root="$(cd "$macos_dir/../.." && pwd)"

cd "$macos_dir"

# --- resolve version ----------------------------------------------------------
resolve_version() {
  if [[ -n "${1:-}" ]]; then
    printf '%s' "$1"
    return 0
  fi
  if [[ -n "${SPOTUIFY_VERSION:-}" ]]; then
    printf '%s' "$SPOTUIFY_VERSION"
    return 0
  fi
  # Pull `version = "x.y.z"` from [workspace.package] in the root Cargo.toml.
  local cargo_toml="$repo_root/Cargo.toml"
  if [[ -f "$cargo_toml" ]]; then
    local v
    v="$(grep -m1 -E '^[[:space:]]*version[[:space:]]*=' "$cargo_toml" | sed -E 's/.*"([^"]+)".*/\1/')"
    if [[ -n "$v" ]]; then
      printf '%s' "$v"
      return 0
    fi
  fi
  return 1
}

if ! VERSION="$(resolve_version "${1:-}")" || [[ -z "$VERSION" ]]; then
  echo "::error:: could not resolve version (pass as arg, set SPOTUIFY_VERSION, or add version to Cargo.toml)" >&2
  exit 64
fi

echo "==> Building Spotuify DMG for version ${VERSION}"

# --- preflight ----------------------------------------------------------------
require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "::error:: required tool '$1' not found on PATH" >&2
    exit 69
  }
}
require xcodegen
require xcodebuild
require hdiutil
require shasum

# --- paths --------------------------------------------------------------------
derived_data="$macos_dir/build/dd"
export_dir="$macos_dir/build/release-export"
dist_dir="$macos_dir/dist"
staging_dir="$macos_dir/build/dmg-staging"
dmg_path="$dist_dir/Spotuify-${VERSION}.dmg"

# --- generate xcode project ---------------------------------------------------
echo "==> Generating Xcode project (xcodegen)"
xcodegen generate

# --- build Release config -----------------------------------------------------
# A plain `build` into a known DerivedData path is the simplest route that
# yields a runnable Spotuify.app, and avoids archive/export-plist signing
# friction for an unsigned distribution. We pin MARKETING_VERSION so the
# bundle's CFBundleShortVersionString matches the DMG name.
echo "==> Building Release configuration (xcodebuild)"
xcodebuild \
  -project Spotuify.xcodeproj \
  -scheme Spotuify \
  -configuration Release \
  -derivedDataPath "$derived_data" \
  -destination 'generic/platform=macOS' \
  MARKETING_VERSION="$VERSION" \
  CODE_SIGN_IDENTITY="-" \
  CODE_SIGNING_REQUIRED=NO \
  CODE_SIGNING_ALLOWED=NO \
  build

products_dir="$derived_data/Build/Products/Release"
app_path="$products_dir/Spotuify.app"

if [[ ! -d "$app_path" ]]; then
  echo "::error:: expected app not found at $app_path after build" >&2
  exit 70
fi
echo "==> Built app: $app_path"

# Stage a clean copy of the app so we never package stale DerivedData siblings.
rm -rf "$export_dir"
mkdir -p "$export_dir"
cp -R "$app_path" "$export_dir/Spotuify.app"
app_path="$export_dir/Spotuify.app"

# Ad-hoc sign so the bundle is internally consistent (unsigned distribution,
# no Developer ID). Best-effort: do not fail the build if codesign is fussy.
if command -v codesign >/dev/null 2>&1; then
  echo "==> Ad-hoc signing app bundle (unsigned distribution)"
  codesign --force --deep --sign - "$app_path" >/dev/null 2>&1 || \
    echo "    (ad-hoc sign skipped; bundle remains unsigned)"
fi

# --- package DMG --------------------------------------------------------------
mkdir -p "$dist_dir"
rm -f "$dmg_path"

if command -v create-dmg >/dev/null 2>&1; then
  echo "==> Packaging DMG with create-dmg"
  # create-dmg returns non-zero (2) when it succeeds but cannot codesign the
  # DMG; that is fine for an unsigned build, so tolerate it as long as the DMG
  # exists afterwards.
  create-dmg \
    --volname "Spotuify ${VERSION}" \
    --app-drop-link 480 170 \
    --icon "Spotuify.app" 140 170 \
    --window-size 640 360 \
    --hide-extension "Spotuify.app" \
    "$dmg_path" \
    "$app_path" || true
  if [[ ! -f "$dmg_path" ]]; then
    echo "::error:: create-dmg did not produce $dmg_path" >&2
    exit 70
  fi
else
  echo "==> create-dmg not found; packaging plain DMG with hdiutil"
  rm -rf "$staging_dir"
  mkdir -p "$staging_dir"
  cp -R "$app_path" "$staging_dir/Spotuify.app"
  ln -s /Applications "$staging_dir/Applications"
  hdiutil create \
    -volname "Spotuify ${VERSION}" \
    -srcfolder "$staging_dir" \
    -fs HFS+ \
    -format UDZO \
    -ov \
    "$dmg_path"
  rm -rf "$staging_dir"
fi

# --- report -------------------------------------------------------------------
if [[ ! -f "$dmg_path" ]]; then
  echo "::error:: DMG not produced at $dmg_path" >&2
  exit 70
fi

size="$(du -h "$dmg_path" | cut -f1 | tr -d '[:space:]')"
sha="$(shasum -a 256 "$dmg_path" | awk '{print $1}')"

echo ""
echo "==> DMG ready"
echo "    path:   $dmg_path"
echo "    size:   $size"
echo "    sha256: $sha"
