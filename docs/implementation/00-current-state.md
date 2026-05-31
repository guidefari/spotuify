# Current State

## What exists

Current spotuify is a Rust workspace with a unified `spotuify`
binary. The binary exposes TUI, CLI, daemon, cache, MCP, lyrics,
analytics, visualization, and maintenance surfaces backed by split
crates for core types, protocol, store, Spotify, player, daemon, CLI,
TUI, audio, system integration, lyrics, sync, and MCP.

Implemented CLI commands:

- `onboard`
- `login`
- `logout`
- `doctor`
- `logs path|tail`
- `config path|init|get|set`
- playback controls: status, play, pause, resume, toggle, next, previous,
  seek, volume, shuffle, repeat, queue, transfer
- browsing/search: search, devices, playlists, playlist tracks,
  recently played, library save/unsave
- daemon lifecycle and IPC-backed one-shot commands
- cache status/reset/repair/reindex
- operations log: ops list/show/undo/redo
- analytics, lyrics, MCP, and visualization commands

Implemented TUI areas:

- search
- grouped search result rendering by media kind, including podcasts/shows
- queue
- playlists
- library
- devices
- diagnostics
- synced lyrics
- persistent bottom player
- optional right rail for queue, lyrics, and contextual key hints
- fullscreen queue and lyrics overlays
- playlist picker modal for add-to-playlist

Implemented Spotify API capabilities:

- playback state
- devices
- queue read
- search tracks/episodes/shows/albums/artists/playlists
- playlists
- recently played
- playlist tracks
- play/pause
- play URI/context
- next/previous
- seek
- volume
- shuffle
- repeat
- add to queue
- append playlist and album selections to the queue by expanding them to
  track batches
- transfer playback
- add to playlist
- save track/episode
- library save/unsave by URI for tracks, albums, episodes, and artist
  follow/unfollow routing where the provider supports it

## Current fixes already applied

- Key-triggered TUI Spotify calls moved off the input loop.
- Empty Spotify write requests send explicit `Content-Length: 0` on
  playback, queue, save/unsave, follow/unfollow, and playlist-unfollow
  paths.
- Spotify OAuth now requests follow scopes, and token status reports missing
  scopes with a relogin hint.
- TUI queue, lyrics, and keymap rails are available without leaving the
  current screen; diagnostics/library refresh planning is automatic.
- TUI add-to-playlist opens an explicit picker instead of guessing a target.
- TUI diagnostics logs are filterable and keyboard-scrollable.
- TUI mouse support covers tabs, rows, progress seeking, right-rail controls,
  bottom-player play/pause, and bottom-player volume scrolling.
- Spotify search limit changed to the current valid max.
- Keychain reads/writes bounded to avoid indefinite hangs.
- Keychain reads that need user approval now latch `AuthRequired` in the
  daemon; auth-error desktop notifications are deduped so an unattended
  prompt does not become a notification storm.
- embedded player device name set to `spotuify-hume`.
- legacy `[spotifyd] device_name` remains accepted as a migration fallback.
- daemon, Unix-socket JSON IPC, workspace split, SQLite cache,
  operation receipts, typed Spotify errors, rate-limit handling,
  MCP stdio/HTTP surfaces, embedded librespot sink-chain wiring, local
  lyrics, and visualization plumbing have landed in later phases.

## Current gaps

- Phase 14 media controls are live for the supported souvlaki path and
  route OS media-key commands through daemon playback requests. Discord
  remains scaffold-only until playback events carry rich metadata.
- Embedded sink visualization is attachable when built with
  `embedded-playback` plus exactly one librespot audio backend feature.
  Native PipeWire visualization remains an optional boundary; the cpal
  loopback path is the implemented default.
- TUI revamp plan items are implemented: playlist picker, full-screen
  queue/lyrics overlays, mouse controls for tabs/rows/progress/rails/bottom
  player, diagnostics log filtering, and playlist/album queue expansion are
  implemented and tested. Future polish should be planned as new scoped work.
- Implementation docs are now execution ledgers: checked items either
  have code/test evidence or are explicitly closed as pivots/follow-ups.

## Immediate risk

The broad surface can regress if plan docs drift from code. Current
verification should favor focused crate tests plus real CLI smoke paths
through daemon/IPC where user-visible behavior is involved.
