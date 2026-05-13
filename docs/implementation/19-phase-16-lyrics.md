# Phase 16 - Lyrics

## Goal

Show synced lyrics in the TUI scrolling with playback position, sourced from Spotify's own backend via embedded librespot's mercury bus, with LRCLIB as a fallback for tracks Spotify doesn't have lyrics for. Persistent local cache.

## Evidence base

| Source | Reference | Notes |
|---|---|---|
| Spotify lyrics via librespot mercury | spotify-player `client/mod.rs:642-661` | `librespot_metadata::Lyrics::get(&session, &track_id)`. Musixmatch-sourced |
| LRCLIB HTTP API | spotatui `infra/network/utils.rs:89-141` | Free community lyrics database; `https://lrclib.net/api/get` with track/artist/duration |
| Line-level alignment via binary-search-by-progress | spotify-player `ui/page.rs:579+` | Each line is `(start_time_ms, text)`; render by finding active index |
| RTL / bidirectional text handling | spotify-player | `unicode-bidi` crate |
| Failure semantics | spotify-player | Treat "not found" as `Ok(None)` rather than error |

## Provider strategy

1. **Spotify (preferred)** — via embedded librespot's mercury bus. Synced. Same source as the official Spotify app. Requires Phase 9's embedded backend; falls back to (2) when running on `--backend spotifyd` or `--backend connect`.
2. **LRCLIB (fallback)** — public HTTP API. Synced when available; plain text when only that exists. No auth required, but they ask for rate-limit etiquette (max ~5 req/s) and a `User-Agent`.
3. **None (last resort)** — show "No lyrics available" with a link/button to suggest manual config.

Provider selection happens per-track and is cached. If Spotify returns "not found", try LRCLIB before giving up.

## Deliverables

### `crates/spotuify-lyrics`
New leaf crate:

```text
crates/spotuify-lyrics/
├── src/
│   ├── lib.rs
│   ├── types.rs           // SyncedLyrics, LyricLine, Provider
│   ├── spotify_provider.rs   // via spotuify-player::mercury_get
│   ├── lrclib_provider.rs
│   ├── parser.rs          // LRC format parser (regex-free, shared between providers)
│   ├── cache.rs           // SQLite-backed persistence
│   └── alignment.rs       // binary-search active-line lookup
```

Depends on `spotuify-core`, `spotuify-store`, `spotuify-player` (for mercury access).

### Wire format

```rust
pub struct SyncedLyrics {
    pub provider: Provider,
    pub track_uri: String,
    pub lines: Vec<LyricLine>,
    pub fetched_at_ms: i64,
    pub synced: bool,                   // false = plain text fallback
    pub language: Option<String>,
    pub source_url: Option<String>,
}

pub struct LyricLine {
    pub start_ms: u64,
    pub text: String,
    pub is_rtl: bool,                   // derived via unicode-bidi
}

pub enum Provider {
    SpotifyMercury,
    Lrclib,
}
```

### LRC parser
Parse `[mm:ss.xx]` and `[mm:ss.xxx]` timestamps. Lines with no timestamp are appended to the previous line's text (Musixmatch's "multi-line per timestamp" pattern). Handles BOM, malformed timestamps (skip with warning), and multiple timestamps per line (duplicate the line).

Reference: spotatui `utils.rs:106-141` is a good template; refine error handling.

### Persistence
- SQLite table `lyrics_cache`:
  ```
  track_uri TEXT PRIMARY KEY
  provider TEXT NOT NULL
  synced INTEGER NOT NULL
  lines_json TEXT NOT NULL
  fetched_at_ms INTEGER NOT NULL
  source_url TEXT
  ```
- TTL: 30 days (configurable). LRCLIB lyrics rarely change; Spotify mercury lyrics also stable.
- Cache miss → fetch → store.
- ETag/If-Modified-Since: LRCLIB doesn't support reliably; rely on TTL.

### LRCLIB etiquette
- Set `User-Agent: spotuify/<version> (https://github.com/bhekanik/spotuify)`.
- Rate-limit to 2 req/s globally with a tokio semaphore.
- Backoff on 429.
- Send `track_name`, `artist_name`, `album_name`, `duration` (seconds) as query params.
- Try `/api/get` first (exact match), `/api/search` second if no result.

### TUI integration
- Lyrics tab/panel on the Player screen.
- Active line centered vertically, surrounding lines fade.
- Bidirectional text rendered correctly (`unicode-bidi`).
- Manual offset adjustment (`+50ms` / `-50ms`) saved per-track in `lyrics_offsets` table.
- "Lyrics" command in command palette opens lyrics panel.
- Falls back gracefully: plain text scroll if not synced.

### Alignment algorithm
Compute `current_position_ms` from Phase 9's `PlayerEvent::PositionChanged` and the derived offset. Find active line by binary search:

```rust
let idx = lyrics.lines.partition_point(|line| line.start_ms <= position_ms);
let active = idx.saturating_sub(1);
```

Re-render only when `active` changes (avoid every-frame re-render).

### CLI commands
- `spotuify lyrics [--track URI] [--format text|jsonl|lrc]`
- `spotuify lyrics fetch <track-uri>` (force refresh)
- `spotuify lyrics export <track-uri>` writes LRC file
- `spotuify lyrics provider --set spotify|lrclib|auto` (default `auto`)
- `spotuify lyrics offset <track-uri> +50ms` (save per-track timing tweak)

### MCP integration
- `lyrics` tool in MCP server (Phase 8) returns synced lyrics for current or specified track.

## Work items

1. New `crates/spotuify-lyrics` crate.
2. LRC parser with tests for: BOM, 2/3-digit ms, duplicate timestamps, malformed lines.
3. SpotifyMercuryProvider using `spotuify-player::mercury_get("hm://lyrics/v1/track/{id}")`.
4. LrclibProvider with HTTP client, etiquette wrapper.
5. SQLite migration + cache layer.
6. unicode-bidi integration for RTL.
7. TUI panel widget; bind to ratatui layout in Player screen.
8. Manual offset persistence.
9. CLI commands.
10. MCP tool.
11. Doctor reports lyrics provider config, cache size.

## Verification

- Spotify track with known lyrics on `--backend embedded`: synced lyrics scroll in time.
- Same track on `--backend spotifyd` (no mercury): falls back to LRCLIB, still synced.
- Arabic/Hebrew track: RTL rendering correct.
- Track with no Spotify lyrics, no LRCLIB entry: "No lyrics available" shown without errors.
- Offline mode (network down): cached lyrics render; missing ones show "Offline".
- 100 rapid track changes (test playlist): no race conditions, cache fills correctly, no orphan rows.
- `spotuify lyrics export <uri>` produces a valid LRC file that mpv or VLC can render.
- Manual offset `+200ms` persists across daemon restart.

## Definition of done

Lyrics appear in the Player TUI panel, scroll with the song, support RTL languages, fall back gracefully between providers, cache locally, and survive daemon restart. CLI exposes the same lyrics for scripts. MCP tool returns lyrics for agent consumption. spotuify becomes the only Rust Spotify TUI with both Spotify-mercury and LRCLIB providers cached locally with manual offset control.
