#!/usr/bin/env bash
set -euo pipefail

repo="planetaryescape/spotuify"
install_dir="${SPOTUIFY_INSTALL_DIR:-$HOME/.local/bin}"
version="${SPOTUIFY_VERSION:-latest}"

usage() {
  cat <<'EOF'
usage: install.sh [--version <version>] [--dir <install-dir>]

Installs the spotuify release archive for this OS/arch and verifies it against
the .sha256 file published with the GitHub Release before installing.

Environment:
  SPOTUIFY_VERSION      Release version, e.g. v0.1.24. Defaults to latest.
  SPOTUIFY_INSTALL_DIR  Install directory. Defaults to ~/.local/bin.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      version="${2:?--version requires a value}"
      shift 2
      ;;
    --dir)
      install_dir="${2:?--dir requires a value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required" >&2
    exit 69
  fi
}

need curl
need tar

case "$(uname -s):$(uname -m)" in
  Darwin:arm64) platform="macos-aarch64" ;;
  Darwin:x86_64) platform="macos-x86_64" ;;
  Linux:x86_64) platform="linux-x86_64" ;;
  *)
    echo "no prebuilt spotuify archive for $(uname -s) $(uname -m)" >&2
    echo "try: cargo install --git https://github.com/$repo --locked spotuify" >&2
    exit 69
    ;;
esac

if [[ "$version" == "latest" ]]; then
  latest_url="$(curl -fsSIL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")"
  version="${latest_url##*/}"
fi

release_version="${version#v}"
tag="v${release_version}"
archive="spotuify-v${release_version}-${platform}.tar.gz"
base_url="https://github.com/$repo/releases/download/$tag"

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

curl -fL --proto '=https' --tlsv1.2 -o "$tmpdir/$archive" "$base_url/$archive"
curl -fL --proto '=https' --tlsv1.2 -o "$tmpdir/$archive.sha256" "$base_url/$archive.sha256"

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmpdir" && sha256sum -c "$archive.sha256")
elif command -v shasum >/dev/null 2>&1; then
  (cd "$tmpdir" && shasum -a 256 -c "$archive.sha256")
else
  echo "sha256sum or shasum is required to verify $archive" >&2
  exit 69
fi

tar -xzf "$tmpdir/$archive" -C "$tmpdir"
if [[ ! -x "$tmpdir/spotuify" ]]; then
  echo "archive did not contain an executable spotuify binary" >&2
  exit 65
fi

mkdir -p "$install_dir"
install -m 0755 "$tmpdir/spotuify" "$install_dir/spotuify"
echo "installed spotuify $tag to $install_dir/spotuify"
