# Spotuify visual revamp — comprehensive plan

## Why this exists

The structural revamp (bottom transport, search groups, lyrics rail, mouse, auto-sync) already shipped on the branch and passes its tests. The user opened the app expecting the Spotify-desktop hero treatment they screenshotted as a target and saw essentially the same UI. The tests verify "region X exists at row N." They do not verify "this looks like a music player you'd want to use."

This plan enumerates **every visual surface** in the TUI, names the gap between "what's there now" and "the target," lists the atomic change that closes the gap, and pins a verification step that requires me to look at a rendered frame before claiming done.

## Verification rule (non-negotiable)

For every surface in this plan:

1. Render the surface via `TestBackend` at `140x40` (representative terminal) with a known sample state, dump the buffer as text to `docs/research/visual-snapshots/<surface>.txt`.
2. Inspect the dump myself first. If it doesn't look like a music player, the change isn't done — even if the structural test passes.
3. Surface the dumps to the user so they can eyeball before I close a task.

No task is "done" until the snapshot is captured and the visual feels closer to the target than the baseline. Sycophantic structural assertions are not enough.

## Surfaces and target state

| # | Surface | Now | Target |
|---|---|---|---|
| 1 | Bottom transport (always visible) | `[18-col art] [flex track text] [26-col control text]` — art tiny, title regular-bold, controls are plain unicode glyphs. | `[18×6 art with halfblock or gradient] [big-text title via tui-big-text, artist+album below, gauge below] [transport chips as inverted buttons + volume bar]`. Active state (shuffle/repeat/like) uses green chip bg. |
| 2 | Player screen body (Screen::Player) | Track text on left, queue preview on right. Spectrum only when `viz_enabled && player_large`. | Hero pane: big-text title (3-row), artist line, big progress gauge with seek thumb. Right column: 12-band spectrum (default on) above queue preview. Compact `z` flips to single-row mini info + queue list. |
| 3 | Search screen | Single flat list when small. Grouped (Tracks/Artists/Playlists/Podcasts) when wide. | Always grouped (down to 80 cols). Each group is a bordered card with kind icon + count badge in the title. Focused group has GREEN border + inverted title chip. Per-group contextual hints update the hint bar. |
| 4 | Library screen | Flat list with filter bar. | Same flat list (no album-grid in TUI), but rows get cover thumbnails (3 cols), kind glyph, subtitle on second line. Empty state names the in-flight sync explicitly, not generic prose. |
| 5 | Playlists screen | Two-pane: left names, right tracks. | Left pane: each playlist as a card row with cover thumbnail, name (bold), `N tracks · owner` (muted), pinned chip if owner == me. Right pane: track rows with index + name + duration. |
| 6 | Queue screen | Now-playing header + filter + upcoming list. | Now-playing card at top (album art + big-text title), then `Up next` section with rows, then `History` collapsible. Drag-handle glyph hints reorderable rows (future). |
| 7 | Devices screen | Table: Device, Type, State, Volume. | Card list with per-device-kind icon (🖥 computer, 📱 phone, 🔊 speaker, 📺 tv, 🎧 headphones), name (bold), state chip (GREEN `playing` / MUTED `idle` / RED `restricted`), volume mini-bar. Stable identity-ordered. |
| 8 | Diagnostics screen | Two columns: health/findings + cache/logs/ops. Filter `/`. | Same columns, but each block has a section title chip, syntax-highlighted log lines (timestamp MUTED, level chip colored by severity, message TEXT), scrollbar on the right of logs, filter input docked at the bottom of the log block with a cursor. |
| 9 | Lyrics rail (right) | Plain text list, active line bold. | Active line in big-bold (1.5× via spans), prev/next lines fade to MUTED, scroll auto-centers active. Header row: cover thumb + track name. Footer row: provider name + offset (e.g. `LRCLIB · +50ms`). Empty state: "instrumental — no synced lyrics for this track". |
| 10 | Queue rail (right) | Plain item list. | Row: small cover thumbnail (3-col) + name (bold) + subtitle (muted) + duration. Now-playing row highlighted with GREEN chip on the left. |
| 11 | Hints rail (right) | Plain list of "key: label". | Section headers (Navigation, Playback, Selection, Help), each shortcut as `[K]` inverted chip + label, scrollable. |
| 12 | Hint bar (status row, always visible) | Single line `key: label  key: label  …`, displaced by toast/banner/spinner. | Dedicated 1-row bar at the very bottom, never displaced. Toast/banner moves to its own ephemeral row above it. Each shortcut renders as `[K]` inverted chip + ` label ` with `·` separators. Truncate with ellipsis at width. |
| 13 | Playlist-picker modal | Centered list of playlists, `[x]` markers. | Modal title chip, search input at top, filtered list with cover thumbs, multi-select markers as filled green circles, footer `Space toggle · Enter add · Esc cancel`. |
| 14 | Confirm modal | Red border, body, `[y] yes [n] no Esc cancel`. | Same shape but `Yes` and `No` rendered as button chips. Title chip in red. |
| 15 | Error modal | Centered red modal with error text. | Add a category icon to the title (`⚠` for 4xx, `✖` for 5xx, `⚡` for network). Body breaks lines on `:`. Footer: `Esc dismiss · ? help`. Full error chain shown. |
| 16 | Banner (status row) | Inline string above the status. | Dedicated banner row at the top of the body region (above the body, below tabs) with severity-colored left bar, icon, message, and right-aligned `R` chip when actionable. Doesn't crowd the toast row. |
| 17 | Help overlay (`?`) | Plain rows of `key  label`. | Two-column grid: shortcut chips on the left, labels on the right. Searchable header bar with cursor. Tabbed sections (Navigation, Playback, Selection, Misc). |
| 18 | Command palette (`Ctrl-p`) | List of actions with shortcut+label. | Input at top with cursor, command rows with `[shortcut]` chips and category badges, footer hint `Enter run · Esc cancel`. |
| 19 | Empty states (every screen) | Static text. | Throbber spinner while in-flight, friendly multi-line copy with the next CLI to run, GREEN accent. |
| 20 | Focus mode (`F`) | Whichever rail is open expands to fullscreen. | Fullscreen art (centered, half-width), big-text title above, big subtitle, gauge with seek thumb at the bottom, no chrome. Hint bar reduces to a single line at the very bottom. |
| 21 | Color palette / typography | `BG TEXT GREEN MUTED RED PANEL WARN` only. | Add `ACCENT` (cyan-ish for non-Spotify-branded affordances), `DIM_BORDER` (subtler than current), `CHIP_BG`/`CHIP_FG` for inverted chips. Title chips, section chips, key chips all reuse the chip pair. |
| 22 | Tabs / sidebar (top of screen) | Tab strip with `1-8` and labels. | Numeric prefix as a small chip, current tab fully inverted, others muted, separators are thin `│` dim. |

## Atomic execution order

Each step ends with a snapshot capture (`docs/research/visual-snapshots/<step>.txt`). Steps that fail visual review get reworked, not closed.

1. **Add helpers** — `tui-big-text` (already added), introduce `key_chip(text)`, `section_chip(text)`, `card_block(title)` helpers in `widgets/style.rs`. No layout change. Snapshot: render a row of chips to confirm they look like buttons.
2. **Hint bar rebuild (#12)** — dedicated row, key chips, toast on separate row. Snapshot: bottom 2 rows of the TUI at 140 cols.
3. **Bottom transport (#1)** — chip-styled controls, volume mini-bar, layout rebuild. Snapshot: full transport region with a track playing.
4. **Big-text title in transport** — pulls `tui-big-text::BigText` for `item.name`. Snapshot: same region with a real track name.
5. **Album art gradient fallback (#21)** — deterministic 2-color gradient on `track_id`. Snapshot: with and without cover bytes.
6. **Player screen body (#2)** — hero left + spectrum/queue right. Snapshot: full body at 140x40 with sample track + sample queue.
7. **Search groups card upgrade (#3)** — bordered cards with kind icons + count chips. Snapshot: search results with all 6 kinds.
8. **Library row upgrade (#4)** — cover thumbs + two-line rows. Snapshot.
9. **Playlists card list (#5)** — card rows. Snapshot.
10. **Queue screen (#6)** — now-playing card + sections. Snapshot.
11. **Devices card list with kind icons (#7)** — Snapshot.
12. **Diagnostics polish (#8)** — section chips + log severity chips + scrollbar. Snapshot.
13. **Lyrics rail polish (#9)** — header + active-line emphasis + footer. Snapshot.
14. **Queue rail polish (#10)** — thumbs + now-playing chip. Snapshot.
15. **Hints rail polish (#11)** — sections + chips. Snapshot.
16. **Modals — playlist picker, confirm, error (#13-#15)** — title chips + button chips + categories. Snapshot per modal.
17. **Banner row separation (#16)** — dedicated row, severity bar, action chip. Snapshot.
18. **Help overlay (#17)** — two-column grid + search + sections. Snapshot.
19. **Command palette (#18)** — input + chips + footer. Snapshot.
20. **Empty states (#19)** — throbber + copy + accent across every screen. Snapshot of each empty state.
21. **Focus mode (#20)** — fullscreen art + big-text. Snapshot.
22. **Palette + typography hierarchy (#21)** — introduce `ACCENT`, `DIM_BORDER`, `CHIP_BG`/`CHIP_FG`. Use across all surfaces. Snapshot a few representative screens to confirm the palette reads coherently.
23. **Tab strip / sidebar (#22)** — chip-styled. Snapshot.

## Acceptance

After step 23, all 22 snapshots live in `docs/research/visual-snapshots/`. User opens the app and the dominant impression is "this looks like a music player," not "this looks like a generic ratatui demo." If a snapshot looks wrong, that step gets reworked before moving on.

## Out of scope

- Truecolor only when the terminal supports it (will not detect; assume 24-bit).
- Mouse drag for seek (hit-region only).
- Animated transitions (`tachyonfx`) — defer unless trivial.
- Theming/config — palette is hardcoded until the rest lands.

## What's already shipped this branch (do not redo)

- Three-zone layout (body / transport / status) — present, but transport is the part being upgraded.
- Search grouping by kind — present, but visual style is the gap.
- Lyrics rail (`L`), queue rail (`Q`), hints rail (`H`), fullscreen (`F`) — all wired, all need visual polish.
- Mouse hit-test framework — wired, controls just need to read as buttons so users know to click.
- Library/Diagnostics auto-sync — wired, the loading copy needs the throbber + accent treatment.
- API/auth fixes (411, scope-reauth, error-modal lifetime) — done this session.

## When to deviate

If a step turns out to need a different ratatui crate, swap and note it in the snapshot's filename. Don't silently extend scope; surface the swap. If the user wants step ordering changed, follow them.
