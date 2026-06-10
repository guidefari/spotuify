---
title: "Browse an Artist's Discography"
description: "List followed artists, browse a full discography, and filter to saved albums."
---

Spotify hides an artist's full catalog a few screens deep and gives you no
"only what I've saved" filter. spotuify puts the whole discography behind one
command and one toggle.

## List the artists you follow

```bash
spotuify artist followed
spotuify artist followed --format ids
```

Followed artists come from the local cache (synced from `/me/following`), so
the list is instant after the first sync. Pipe `--format ids` into the next
command to drive a whole flow from the shell.

## Browse one artist's discography

```bash
spotuify artist albums spotify:artist:36QJpDe2go2KgaRleHCDTp
```

The daemon fetches every release group (albums, singles, compilations, and
appears-on), de-duplicates re-releases, and tags each album with whether it is
already in your library. The table view groups the releases and prints a count
footer:

```text
Albums (4)
  ✓ 1981 Never Too Much        spotify:album:...
    1982 Forever, for Always... spotify:album:...

Singles & EPs (2)
    2023 Best of Love           spotify:album:...

3 albums • 1 in library
```

The `✓` marks albums you have saved.

## Filter to your library, or to specific groups

Show only albums already saved to your library:

```bash
spotuify artist albums spotify:artist:36QJpDe2go2KgaRleHCDTp --library-only
```

Restrict to one or more release groups (`album`, `single`, `compilation`,
`appears-on`); repeat `--group` to combine them:

```bash
spotuify artist albums spotify:artist:36QJpDe2go2KgaRleHCDTp --group album --group single
```

Both flags are local filters over the data the daemon already returned, so they
do not trigger another Spotify call.

## Discovery: related artists and radio

Two Mercury-backed discovery commands ride the daemon's librespot session
(the Web API equivalents were deprecated in 2024):

```bash
# Artists related to a given one
spotuify artist related spotify:artist:4uLU6hMCjMI75M1A2tKUQC --format json

# Radio station seeded by any track/artist/album/playlist URI.
# --dry-run previews the resolved tracks without queueing them.
spotuify radio start spotify:track:... --dry-run
spotuify radio start spotify:artist:...
```

Without `--dry-run`, `radio start` queues the resolved station onto the active
device. These endpoints are reverse-engineered; if Spotify changes them the
commands return an "endpoint may have changed" error rather than failing
loudly. Agents reach the same via the MCP `related_artists` / `radio_start`
tools.

## Machine-readable output

```bash
spotuify artist albums spotify:artist:36QJpDe2go2KgaRleHCDTp --format json
```

Each row carries `album_group` and `in_library` so scripts can section and
filter the same way the table does:

```json
{
  "uri": "spotify:album:...",
  "name": "Never Too Much",
  "subtitle": "Luther Vandross",
  "kind": "album",
  "album_group": "album",
  "in_library": true,
  "release_date": "1981-08-11"
}
```

For example, count saved albums for an artist:

```bash
spotuify artist albums spotify:artist:36QJpDe2go2KgaRleHCDTp --format json \
  | jq '[.[] | select(.in_library)] | length'
```

## In the TUI

Open `spotuify`, go to Library (`3`), pick the Artists view, and press `Enter`
on an artist to open the discography overlay. Releases appear in grouped
sections on the left and the focused album's tracks on the right.

- `L` toggles between all releases and only those in your library.
- `Tab` swaps focus between the album list and the track list.
- `Enter` plays the focused album or track; `Esc` closes the overlay.

The macOS app mirrors this: an Artists item in the sidebar lists followed
artists, and each artist page has an "All / In Library" segmented control above
the grouped release sections.

## See Also

- [Artist Albums CLI](/reference/cli/artist-albums/)
- [Artist Followed CLI](/reference/cli/artist-followed/)
- [JSON Output](/reference/json-output/)
- [Keybindings](/reference/keybindings/)
