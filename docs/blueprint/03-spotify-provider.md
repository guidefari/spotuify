# spotuify - Spotify Provider

## Provider role

The Spotify provider is the adapter between spotuify's internal model and Spotify's Web API plus Spotify Connect behavior.

It should isolate Spotify-specific quirks from the daemon, CLI, and TUI.

## API categories

### Auth

- OAuth PKCE for local CLI/TUI use.
- Refresh token stored in system keyring.
- Scope requests should be minimal and explained.

### Catalog

- tracks
- albums
- artists
- playlists
- shows
- episodes
- audiobooks
- chapters

### User library

- saved tracks
- saved albums
- saved episodes
- saved shows
- saved audiobooks
- followed artists
- followed playlists

### Playlists

- list current user playlists
- create playlist
- update metadata
- list items
- add items
- remove items
- reorder items
- replace items
- cover image support later

### Player

- playback state
- currently playing
- devices
- transfer playback
- play/resume
- pause
- next/previous
- seek
- shuffle
- repeat
- volume
- queue read
- queue add

### Personalization

- recently played
- top tracks
- top artists

## Known Spotify limitations

- The Web API does not stream audio.
- A real Spotify Connect device must exist for playback.
- Queue removal and queue reorder are not exposed by the Web API.
- Official lyrics are not exposed by the Web API. Lyrics require an optional external provider or no feature.
- Playback control requires Premium.
- Some endpoints and fields have changed under Spotify's 2026 developer access changes.
- Search `limit` must respect Spotify's current max.
- Rate limits must respect `Retry-After`.

## Device strategy

Preferred order:

1. Active unrestricted device.
2. The daemon's own embedded-device id, when known.
3. Configured device name, currently `spotuify-hume` for this machine.
4. Device name containing `spotuify` or `librespot`.
5. Name-substring overlap with the configured preferred name.
6. Helpful error with `spotuify devices` output.

Do not fall back to an unrelated unrestricted device merely because it is visible. Playback is a mutation; if the preferred target is unavailable, fail with remediation rather than surprise-starting another room or account device.

## Embedded librespot role

The daemon owns an embedded librespot session and registers spotuify as a local Spotify Connect device.

Closing the TUI must never kill playback. `spotuify daemon` owns the player lifecycle and exposes device/playback state to CLI, TUI, MCP, and agents.

## Error normalization

Provider errors should map into typed categories:

- `AuthExpired`
- `AuthDenied`
- `PremiumRequired`
- `NoActiveDevice`
- `DeviceUnavailable`
- `RateLimited`
- `NetworkTimeout`
- `SpotifyServerError`
- `DecodeError`
- `UnsupportedCapability`

CLI and TUI should render these with remediation commands.
