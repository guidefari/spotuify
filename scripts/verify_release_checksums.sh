#!/usr/bin/env bash
set -euo pipefail

artifacts_dir="${1:-}"
if [[ -z "$artifacts_dir" ]]; then
  echo "usage: $0 <artifacts-dir>" >&2
  exit 64
fi

if [[ ! -d "$artifacts_dir" ]]; then
  echo "artifacts directory not found: $artifacts_dir" >&2
  exit 66
fi

if command -v sha256sum >/dev/null 2>&1; then
  verify=(sha256sum -c)
elif command -v shasum >/dev/null 2>&1; then
  verify=(shasum -a 256 -c)
else
  echo "sha256sum or shasum is required to verify release artifacts" >&2
  exit 69
fi

found=false
while IFS= read -r -d '' checksum_file; do
  found=true
  archive="${checksum_file%.sha256}"
  if [[ ! -f "$archive" ]]; then
    echo "missing archive for checksum: $checksum_file" >&2
    exit 66
  fi
  (
    cd "$(dirname "$checksum_file")"
    "${verify[@]}" "$(basename "$checksum_file")"
  )
done < <(find "$artifacts_dir" -type f -name '*.sha256' -print0 | sort -z)

if [[ "$found" != true ]]; then
  echo "no .sha256 files found in $artifacts_dir" >&2
  exit 66
fi
