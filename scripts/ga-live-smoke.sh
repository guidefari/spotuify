#!/usr/bin/env bash
set -euo pipefail

SPOTUIFY_BIN="${SPOTUIFY_BIN:-spotuify}"

usage() {
  cat <<'EOF'
Usage:
  SPOTUIFY_BIN=spotuify scripts/ga-live-smoke.sh

Local target builds default to the dev instance. To test a target build against
the real release account/config, opt into the prod instance explicitly:

  SPOTUIFY_ALLOW_PROD_INSTANCE_FROM_TARGET=1 SPOTUIFY_INSTANCE=spotuify SPOTUIFY_BIN=./target/release/spotuify scripts/ga-live-smoke.sh

Default checks are live but read-only:
  doctor, daemon restart/status, devices, search, queue, playlist dry-run.

Opt-in mutation checks:
  SPOTUIFY_GA_LIVE_PLAYBACK=1   run play, queue add, next
  SPOTUIFY_GA_LIVE_PLAYLIST=1   create a temporary playlist and undo it

This script intentionally does not run from CI. It is a human/agent
release gate for the signed binary against a real Spotify account.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

run() {
  {
    printf '+ %q' "$SPOTUIFY_BIN"
    printf ' %q' "$@"
    printf '\n'
  } >&2
  "$SPOTUIFY_BIN" "$@"
}

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/spotuify-ga-smoke.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

run doctor
run daemon restart
run daemon status --format json
run devices --format json
run search "luther vandross" --type track --format json
run queue --format json

plan="$tmp_dir/plan.json"
resolved="$tmp_dir/resolved.jsonl"
run playlist plan "GA smoke one upbeat soul track" --format json >"$plan"
run resolve-tracks --from "$plan" --format jsonl >"$resolved"
run playlist create "spotuify GA smoke dry-run" --from "$resolved" --dry-run --format json

if [[ "${SPOTUIFY_GA_LIVE_PLAYBACK:-}" == "1" ]]; then
  run play "luther vandross"
  run queue add --search "never too much" --format json
  run next --format json
fi

if [[ "${SPOTUIFY_GA_LIVE_PLAYLIST:-}" == "1" ]]; then
  playlist_name="spotuify GA smoke $(date +%Y%m%d%H%M%S)"
  run playlist create "$playlist_name" --from "$resolved" --yes --format json
  run ops undo --dry-run --format json
  run ops undo --yes --format json
fi

printf 'GA live smoke completed.\n'
