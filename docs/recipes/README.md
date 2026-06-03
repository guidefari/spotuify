# spotuify shell-hook recipes

Spotuify emits a `listen_qualified` event every time a track crosses the
qualification threshold (`audible_ms >= max(30s, min(50% of duration, 4min))`).
By pointing `analytics.hook_command` at one of these scripts in
`~/.config/spotuify/spotuify.toml`, you can bridge listens into your
external scrobbler of choice without spotuify shipping provider-specific
integration in-tree.

```toml
[analytics]
hook_command = "/path/to/spotuify/docs/recipes/scrobble-listenbrainz.sh"
hook_timeout_ms = 5000
```

The hook receives every qualified listen as environment variables:

| Variable | Description |
| --- | --- |
| `SPOTUIFY_TRACK_URI` | `spotify:track:…` URI of the qualifying track |
| `SPOTUIFY_DURATION_MS` | Total track duration in ms |
| `SPOTUIFY_AUDIBLE_MS` | Audible time accrued (excludes paused intervals) |
| `SPOTUIFY_ARTIST_URI` | `spotify:artist:…` URI (may be empty) |
| `SPOTUIFY_ALBUM_URI` | `spotify:album:…` URI (may be empty) |

Hooks are fire-and-forget: spotuify spawns them in the background with a
configurable hard timeout (`hook_timeout_ms`, default 5s), and any
non-zero exit or timeout is logged at `warn` but does not affect
playback.

## Recipes in this directory

- `scrobble-listenbrainz.sh` — POST to ListenBrainz `submit-listens`.
  Requires `LISTENBRAINZ_TOKEN` in the hook's environment.
- `scrobble-lastfm.sh` — sketch using Last.fm `track.scrobble` (needs
  HMAC signing — see comments).
- `notify-discord-listening.sh` — POST a now-playing embed to a Discord
  webhook (`DISCORD_WEBHOOK_URL`).

## Why this design

Bundling Last.fm / ListenBrainz authentication in spotuify itself would
expand the credential surface (more stored secrets) and tie us
to whichever scrobblers we picked. Punting to shell hooks keeps the
core daemon focused on Spotify and lets the community ship recipes
without touching Rust.

If you write a useful hook, PRs adding new scripts here are welcome.
