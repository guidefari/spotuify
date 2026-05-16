---
title: Spotuify visual revamp — snapshots
---

Every step in `docs/research/2026-05-15-visual-revamp.md` ships a
`TestBackend` snapshot here so we can inspect the layout without
running the binary. Real terminals add colour and chip-styling that
a symbol-only dump can't show; the structure, hierarchy, and copy are
what the snapshots prove.

| Step | File | What it verifies |
|------|------|------------------|
| 01 | [01-chips.txt](./01-chips.txt) | Key/section/state/button chips + card_block / focused_card_block helpers |
| 02 | [02-hint-bar.txt](./02-hint-bar.txt) | Hint bar always present, never displaced by toast |
| 03 | [03-transport.txt](./03-transport.txt) | Bottom transport chip buttons + volume bar |
| 04 | [04-bigtext-title.txt](./04-bigtext-title.txt) | tui-big-text track title in transport |
| 05 | [05-art-fallback.txt](./05-art-fallback.txt) | Deterministic 2-colour gradient art fallback |
| 06 | [06-player-body.txt](./06-player-body.txt) | Player tab body — hero card + spectrum + queue |
| 07 | [07-search-groups.txt](./07-search-groups.txt) | Search results as 6 grouped cards (1- or 2-row grid) |
| 08 | [08-library.txt](./08-library.txt) | Library 2-line rows with marker + duration |
| 09 | [09-playlists.txt](./09-playlists.txt) | Playlists list with art markers + owner |
| 10 | [10-queue.txt](./10-queue.txt) | Queue Now-Playing card + Up Next card with media rows |
| 11 | [11-devices.txt](./11-devices.txt) | Devices card list with kind icons + state chips + volume bar |
| 12 | [12-diagnostics.txt](./12-diagnostics.txt) | Diagnostics section chips + log severity chips |
| 13 | [13-lyrics.txt](./13-lyrics.txt) | Lyrics view: header thumb + active-line emphasis + provider footer |
| 14 | [14-queue-rail.txt](./14-queue-rail.txt) | Queue rail with section chips and `+N more` overflow |
| 15 | [15-hints-rail.txt](./15-hints-rail.txt) | Hints rail grouped by category with key chips |
| 16 | [16-modals.txt](./16-modals.txt) | Playlist picker / confirm / error modals (chip-titled, button-chip footers) |
| 17 | [17-banner.txt](./17-banner.txt) | Banners with severity chip + inline `R` chip for ScopeReauthRequired |
| 18 | [18-help.txt](./18-help.txt) | Help overlay two-column grid + filter |
| 19 | [19-palette.txt](./19-palette.txt) | Command palette with input cursor + key chips + category chips |
| 20 | [20-empty-states.txt](./20-empty-states.txt) | Every screen's empty state with throbber/copy/accent |
| 21 | [21-focus.txt](./21-focus.txt) | Focus / fullscreen mode (Full PixelSize big-text + giant gradient art) |
| 22 | [22-palette.txt](./22-palette.txt) | Colour roles and typography hierarchy |
| 23 | [23-tabs.txt](./23-tabs.txt) | Tabs with numeric chips + dim dividers + active inverted |

## How to look at any of these live

```bash
# Render any snapshot to your terminal via the test that produced it.
cargo test -p spotuify-tui --lib snapshot_06_player_body -- --nocapture
```

Substitute the snapshot number (`01`, `02`, …, `23`) for the matching
test name. Pipe through your terminal as-is to see the colours that
the symbol-only dump strips.

## Live sign-off

The whole revamp ships behind one user-facing change: just launch
`spotuify`. The bottom transport is always there, the Player tab opens
to the hero pane, every other screen has its new chrome. Compact mode
(`z`), focus mode (`F`), lyrics rail (`L`), queue rail (`Q`), hint
rail (`H`), command palette (`Ctrl-p`), and help (`?`) all use the
shared chip/card vocabulary.

If anything feels off — copy, hierarchy, spacing — point at the
snapshot file and we iterate from there.
