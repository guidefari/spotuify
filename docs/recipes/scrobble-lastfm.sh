#!/usr/bin/env bash
#
# spotuify shell-hook recipe: scrobble a qualified listen to Last.fm.
#
# Last.fm's track.scrobble endpoint requires an MD5-signed payload + an
# active session key (obtained via the desktop auth flow). This recipe
# is a sketch — fill in your `api_sig` signing logic (or use a helper
# like the `pylast` CLI) before relying on it.
#
# Required environment:
#
#   LASTFM_API_KEY       — from https://www.last.fm/api/account/create
#   LASTFM_API_SECRET    — paired secret used to sign the request
#   LASTFM_SESSION_KEY   — desktop-auth session key (long-lived)
#
# Spotuify passes the same env vars as the ListenBrainz recipe:
#
#   SPOTUIFY_TRACK_URI, SPOTUIFY_DURATION_MS, SPOTUIFY_AUDIBLE_MS,
#   SPOTUIFY_ARTIST_URI, SPOTUIFY_ALBUM_URI
#
# Spotuify only emits URIs to the hook; you'll want to enrich with
# display names before calling Last.fm. The cleanest path is:
#
#   1. Cache `(track_uri → name, artist_name, album_name)` in your
#      shell from `spotuify analytics top --format json --limit 1`
#      output, OR
#   2. Resolve names via the Spotify Web API directly from the script,
#      using the spotuify-issued token.
#
# This stub demonstrates the request shape only; expect to refine.

set -euo pipefail

: "${LASTFM_API_KEY:?missing LASTFM_API_KEY}"
: "${LASTFM_API_SECRET:?missing LASTFM_API_SECRET}"
: "${LASTFM_SESSION_KEY:?missing LASTFM_SESSION_KEY}"
: "${SPOTUIFY_TRACK_URI:?missing SPOTUIFY_TRACK_URI from spotuify hook}"

ts="$(date +%s)"
track_id="${SPOTUIFY_TRACK_URI##*:}"
artist_id="${SPOTUIFY_ARTIST_URI##*:}"

# api_sig = md5(<all params concatenated as key+value>, then api_secret)
# (Implement in your favourite shell; here we just echo the request shape.)
cat <<EOF >&2
[lastfm scrobble stub]
  artist=${artist_id:-unknown}
  track=${track_id}
  timestamp=${ts}
  duration=${SPOTUIFY_DURATION_MS:-0}
  api_key=${LASTFM_API_KEY}
  sk=${LASTFM_SESSION_KEY}
  method=track.scrobble
  api_sig=<sign this payload with LASTFM_API_SECRET and md5>
EOF

# Replace the echo above with a real POST when you wire signing:
#
#   curl --silent --fail \
#     "https://ws.audioscrobbler.com/2.0/" \
#     -d "method=track.scrobble" \
#     -d "artist=${artist_id}" \
#     -d "track=${track_id}" \
#     -d "timestamp=${ts}" \
#     -d "api_key=${LASTFM_API_KEY}" \
#     -d "sk=${LASTFM_SESSION_KEY}" \
#     -d "api_sig=${signature}"
