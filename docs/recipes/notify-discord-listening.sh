#!/usr/bin/env bash
#
# spotuify shell-hook recipe: POST a "now listening" embed to a Discord
# channel via webhook on every qualified listen.
#
# Usage:
#
#   export DISCORD_WEBHOOK_URL="https://discord.com/api/webhooks/…"
#
# Then in `~/.config/spotuify/spotuify.toml`:
#
#   [analytics]
#   hook_command = "/path/to/notify-discord-listening.sh"
#
# Discord rate-limits webhooks; keep `analytics.hook_command` simple and
# don't fan-out to multiple webhooks from one hook.

set -euo pipefail

: "${DISCORD_WEBHOOK_URL:?DISCORD_WEBHOOK_URL must be set}"
: "${SPOTUIFY_TRACK_URI:?missing SPOTUIFY_TRACK_URI from spotuify hook}"

track_id="${SPOTUIFY_TRACK_URI##*:}"
artist_id="${SPOTUIFY_ARTIST_URI##*:}"
duration_min=$(( ${SPOTUIFY_DURATION_MS:-0} / 60000 ))
audible_min=$(( ${SPOTUIFY_AUDIBLE_MS:-0} / 60000 ))

payload=$(cat <<JSON
{
  "embeds": [
    {
      "title": "Now listening",
      "description": "[Open in Spotify](https://open.spotify.com/track/${track_id})",
      "fields": [
        {"name": "Track", "value": "${track_id}", "inline": true},
        {"name": "Artist", "value": "${artist_id:-unknown}", "inline": true},
        {"name": "Audible / Duration", "value": "${audible_min}m / ${duration_min}m", "inline": false}
      ],
      "color": 1947988
    }
  ]
}
JSON
)

curl --silent --fail \
  -H "Content-Type: application/json" \
  -X POST "${DISCORD_WEBHOOK_URL}" \
  -d "${payload}" \
  || { echo "Discord webhook POST failed" >&2; exit 1; }
