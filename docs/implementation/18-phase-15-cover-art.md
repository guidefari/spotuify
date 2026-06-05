# Phase 15 - Cover Art Rendering

## Goal

Show current-track and selected-item artwork inside the TUI on terminals that
support inline images (kitty, iTerm2, WezTerm, Konsole), with graceful
degradation to text-only on others. Reuse the cover cache built for
notifications/MPRIS in Phase 14.

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
- Playlists tab: selected-playlist artwork in the preview area.
- Search/Library: selected album, playlist, show, and episode artwork in the
  preview area when image metadata exists. Row thumbnails remain deferred.

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

1. [x] Add `ratatui-image` to `crates/spotuify-tui` dependencies.
2. [x] Build protocol detection at TUI startup. The current
   implementation uses `ratatui_image::picker::Picker` in `App`, not a
   separate `cover/protocol.rs` module.
3. [x] Implement `crates/spotuify-system/src/cover_cache.rs` (shared with Phase 14).
4. [x] Implement TUI cover rendering with `StatefulProtocol` from
   `ratatui-image`; a separate `CoverArt` widget module was not needed.
5. [x] Wire cover art to the player tab and selected-item previews for
   playlists, albums, shows, and episodes. Now-playing-strip,
   playlist-row, and search-result row thumbnails are intentionally deferred:
   they add visual noise and maintenance cost without a validated need.
6. [x] Pre-cache covers for visible list items in background sync is
   deliberately not shipped with the current player-tab-only cover surface.
   Current behavior fetches on demand through the daemon `CoverArt` request.
   Queue-added tracks now schedule best-effort cover warming because the user
   has already expressed near-term playback intent for those tracks. Broader
   list-thumbnail prefetch remains deferred until there is a validated need.
7. [x] Cache eviction is handled inside `CoverCache` on fetch/stat paths
   using the configured size cap and TTL.
8. [x] Cache status reports cover-cache stats. Terminal protocol
   detection in `doctor` remains a TUI-side follow-up.
9. [x] Config supports `[cache] cover_cache_mb = 200` and
   `[cache] cover_cache_ttl_days = 30`. `[ui] inline_thumbnails` and
   `[ui] cover_size` are deferred with the thumbnail surfaces.

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
- Cache behavior is covered by `spotuify-system` cover-cache tests:
  content-type validation, image decode validation, stale refresh, in-flight
  dedupe, and size-cap eviction.
- TUI player-tab cover rendering uses the shared daemon `CoverArt` request;
  live terminal protocol smoke remains manual.
- `spotuify refresh-media` and TUI `U` re-request current cover art through
  the same daemon `CoverArt` path without clearing the old image first.

## Definition of done

The shipped Phase 15 slice shows player-tab cover art and selected-item
previews through `ratatui-image`, degrades by omitting the image when
loading/rendering fails, and reuses the daemon cover cache across restarts.
Search and Library selected previews include albums, playlists, shows, and
episodes when image metadata exists. Phase 14 notifications and richer row
thumbnails remain follow-ups until track metadata and a clear UX need justify
them. Current-track manual refresh and queue-added cover warming are shipped.
Cache state is reported; terminal protocol reporting is still a TUI-side
diagnostics follow-up.
