#!/usr/bin/env bash
#
# spotuify shell-hook recipe: forward a `listen_qualified` event to
# ListenBrainz via the `submit-listens` REST endpoint.
#
# Usage in ~/.config/spotuify/spotuify.toml:
#
#   [analytics]
#   hook_command = "/path/to/scrobble-listenbrainz.sh"
#   hook_timeout_ms = 5000
#
# Required environment (set in your shell rc or a wrapper script):
#
#   LISTENBRAINZ_TOKEN   — your user token from
#                          https://listenbrainz.org/profile/
#
# Optional:
#
#   LISTENBRAINZ_API     — defaults to https://api.listenbrainz.org
#
# Spotuify passes these as env vars:
#
#   SPOTUIFY_TRACK_URI       spotify:track:…
#   SPOTUIFY_DURATION_MS     total track length in ms
#   SPOTUIFY_AUDIBLE_MS      audible play time accrued
#   SPOTUIFY_ARTIST_URI      spotify:artist:… (may be empty)
#   SPOTUIFY_ALBUM_URI       spotify:album:…  (may be empty)
#
# ListenBrainz wants human-readable track + artist names. Spotuify only
# emits URIs in the hook payload (URIs are stable; display names drift),
# so this script trims the bare ID for now. For richer payloads, run
# `spotuify analytics show <uri>` from within the script and parse the
# JSON — see `notify-discord-listening.sh` for that pattern.

set -euo pipefail

: "${LISTENBRAINZ_TOKEN:?LISTENBRAINZ_TOKEN must be set; see https://listenbrainz.org/profile/}"
: "${SPOTUIFY_TRACK_URI:?missing SPOTUIFY_TRACK_URI from spotuify hook}"
LISTENBRAINZ_API="${LISTENBRAINZ_API:-https://api.listenbrainz.org}"

ts="$(date +%s)"
track_id="${SPOTUIFY_TRACK_URI##*:}"
artist_id="${SPOTUIFY_ARTIST_URI##*:}"
album_id="${SPOTUIFY_ALBUM_URI##*:}"

payload=$(cat <<JSON
{
  "listen_type": "single",
  "payload": [
    {
      "listened_at": ${ts},
      "track_metadata": {
        "track_name": "${track_id}",
        "artist_name": "${artist_id:-unknown}",
        "release_name": "${album_id:-}",
        "additional_info": {
          "duration_ms": ${SPOTUIFY_DURATION_MS:-0},
          "music_service": "spotify.com",
          "origin_url": "https://open.spotify.com/track/${track_id}"
        }
      }
    }
  ]
}
JSON
)

curl --silent --fail \
  -H "Authorization: Token ${LISTENBRAINZ_TOKEN}" \
  -H "Content-Type: application/json" \
  -X POST "${LISTENBRAINZ_API}/1/submit-listens" \
  -d "${payload}" \
  || { echo "ListenBrainz scrobble failed" >&2; exit 1; }
