# Phase 15 - Cover Art Rendering

## Goal

Show album/playlist cover art inside the TUI on terminals that support inline images (kitty, iTerm2, WezTerm, Konsole), with graceful degradation to text-only on others. Reuse the cover cache built for notifications/MPRIS in Phase 14.

## Evidence base

| Approach | Reference | Notes |
|---|---|---|
| `viuer` crate (pinned `=0.9.2`) | spotify-player `Cargo.toml:46-48` | Pinned because newer versions freeze the TUI (issue #899) |
| `ratatui-image` crate | spotatui (current TUI) | More native to ratatui; supports kitty/iTerm/sixel/halfblocks |
| Auto-protocol detection at startup | spotify-player `main.rs:101-108` | Tries kitty → iTerm2 → sixel → fallback |
| Cached re-render on resize only | spotify-player `state/ui/mod.rs` (`last_cover_image_render_info`) | Avoid re-rendering image on every redraw |
| Pixelate fallback | spotify-player `client/mod.rs:1938-1947` | Low-res grid mode when no protocol is available |

## Decision: ratatui-image over viuer

**Use `ratatui-image`.** Reasons:
- Native to ratatui (which we already use), composes as a ratatui Widget.
- Active maintenance; supports kitty, iTerm2, sixel, plus Unicode half-blocks fallback.
- spotify-player's viuer pinning at `=0.9.2` is a real maintenance smell.
- spotatui already uses `ratatui-image` successfully.

Track `ratatui-image` upstream and pin to a known-good minor version with a comment + issue link if a regression appears.

## Deliverables

### Cover-art protocol detection
- At daemon startup (TUI-side, not daemon — protocol detection requires querying the actual terminal):
  - kitty graphics protocol (via `KITTY_WINDOW_ID` env or query response)
  - iTerm2 (`TERM_PROGRAM == "iTerm.app"`)
  - WezTerm (`TERM_PROGRAM == "WezTerm"`)
  - sixel (DA1 response)
  - halfblocks fallback (always available)
- Result cached in `App.image_protocol: Option<Protocol>`.
- Visible in `spotuify doctor` and TUI Diagnostics tab.

### Cover-art widget
- `tui::widgets::CoverArt` ratatui widget.
- Inputs: `image_path: Option<PathBuf>` (file:// or local), `protocol: Protocol`, `target_size: (Width, Height)`.
- Draws once per (image, target_size); caches the rendered terminal sequence.
- On terminal resize: invalidate cache and re-render.
- On track change: load new image via Phase 14's `cover_cache`.

### Cover sources and fallbacks
- Primary: Spotify track/album `images[0].url`.
- Fallback to medium resolution if largest fails.
- Pre-cache covers for visible playlists and recently played items in background sync.
- Placeholder: ASCII art "no image" or just blank if no protocol support.

### Placement in TUI
- Player tab: large cover (~30% width, square aspect) when `player_large` mode.
- Now-playing strip: small thumbnail (4-6 lines) in compact mode.
- Search results: thumbnail per row when image protocol supports it AND user opts in (`[ui] inline_thumbnails = false` default — too busy by default).
- Playlists tab: cover thumbnail next to playlist name.

### Cover cache (shared with Phase 14)
- Single on-disk cache at `~/.cache/spotuify/covers/`.
- Filename: `<spotify-image-id>.jpg` (extract ID from URL: `https://i.scdn.co/image/<id>` → `<id>.jpg`).
- Loaded by `cover_cache::get(url) -> impl Future<PathBuf>`: returns cached if present, fetches and caches if not.
- LRU eviction by file mtime, default cap 200 MB.
- Concurrency: in-flight downloads deduplicated by URL.
- Doctor reports cache size, count, and oldest entry.

### Memory image cache (in-process)
- Decoded image bytes (`image::DynamicImage` or `ratatui-image`'s `Picture`) cached per (image-id, target-size).
- Cap at 50 entries (5 MB ballpark for a TUI). Evict LRU.
- Avoid re-decoding the same image when paginating playlists.

## Work items

1. Add `ratatui-image` to `crates/spotuify-tui` dependencies.
2. Build protocol detection in `crates/spotuify-tui/src/cover/protocol.rs`. Run once at TUI startup.
3. Implement `crates/spotuify-system/src/cover_cache.rs` (shared with Phase 14).
4. Implement `CoverArt` widget in `crates/spotuify-tui/src/widgets/cover_art.rs`.
5. Wire to player tab, now-playing strip, playlists tab, search results (gated).
6. Pre-cache covers for visible items in background sync (Phase 6's sync scheduler).
7. Cache eviction job (daily at idle).
8. Doctor reports protocol detection, cache stats.
9. Config: `[ui] inline_thumbnails = false`, `[ui] cover_size = "medium" | "large"`, `[cache] cover_cache_mb = 200`.

## Verification

- kitty: cover art renders smoothly, doesn't flicker on redraw, redraws on resize.
- iTerm2: same.
- WezTerm with kitty protocol enabled: same.
- macOS Terminal.app (no protocol): falls back to halfblocks unicode; no panics.
- ssh into machine, run spotuify in TERM=screen: falls back cleanly, no escape sequence garbage.
- 100 cover requests in 10s (rapid scroll through playlists): in-flight dedupe keeps to ≤8 concurrent downloads; cache fills correctly.
- Cache hits a 200 MB cap → daemon evicts oldest, never errors.
- Track change with new cover → widget updates within 200ms.
- Resize terminal during playback → cover redrawn at new size on next frame.

## Definition of done

spotuify shows cover art in modern terminals with no flicker, no escape-sequence leak in non-supporting terminals, and graceful degradation. Covers are cached on disk and reused across daemon restarts. Phase 14's notification covers and Phase 15's TUI covers share one cache. Doctor cleanly reports protocol detection and cache state.
