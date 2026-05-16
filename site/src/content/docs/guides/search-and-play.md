---
title: "Search and Play"
description: "Search local cache or Spotify, pick results, and start playback."
---

Search is navigation in `spotuify`: local cache when possible, Spotify when needed, and pipeable IDs for everything else.

## Search the catalog

```bash
spotuify search "burial archangel" --type track
```

What you get: a ranked table of tracks, episodes, shows/podcasts, albums,
artists, or playlists depending on `--type`.

Search in the TUI is grouped by media kind so tracks, artists, albums,
playlists, podcasts, and episodes do not collapse into one undifferentiated
list.

```bash
spotuify search "design matters" --type show
```

## Pick the source

```bash
spotuify search "quiet storm" --source local --format jsonl
spotuify search "quiet storm" --source spotify --format jsonl
spotuify search "quiet storm" --source hybrid --format jsonl
```

Use `local` when you want cached library/search results only. Use `spotify` when discovery matters. `hybrid` is the normal default.

```bash
spotuify search "quiet storm" --source hybrid --format jsonl
```

## Play a result directly

```bash
spotuify search "luther vandross" --type track --play --index 1
```

The index is 1-based. Keep it visible in scripts:

```bash
spotuify search "luther vandross" --type track --limit 5
```

## Pipe IDs

```bash
spotuify search "luther vandross" --type track --format ids \
  | head -n 1 \
  | xargs spotuify play-uri
```

What you get: one stable Spotify URI per line. That is the easiest format for `fzf`, `xargs`, and agents.

```bash
spotuify search "luther vandross" --type track --format ids
```

## Queue from search

```bash
spotuify queue add --search "never too much"
```

Or queue many:

```bash
spotuify search "luther vandross" --type track --format ids \
  | spotuify queue add --format json
```

In the TUI, press `e` on a track, playlist, or album. Tracks append directly;
playlists and albums expand to their tracks and append without wiping the
existing queue.

```bash
spotuify queue
```

## In real life

- Coding and you need energy:

```bash
spotuify play "upbeat focus playlist" --type playlist
```

- You remember a song title badly:

```bash
spotuify search "that one song about homecoming" --type track --source spotify
```

- You want only your cache:

```bash
spotuify search "joni" --source local --format jsonl
```

## See Also

- [Queue and Playlists](/guides/queue-and-playlists/)
- [Search CLI](/reference/cli/search/)
- [Queue Add CLI](/reference/cli/queue-add/)
