# Cached library pagination

## Purpose

Long library surfaces must not render an unbounded cache snapshot. The desktop client requests bounded pages as the user approaches the end of a scroll viewport. The daemon and cache remain authoritative: clients never synthesize membership or invent list rows.

This document records the non-desktop contract used by the library views so CLI, TUI, MCP, and future clients can implement the same behavior.

## IPC requests

### `SavedAlbums`

`Request::SavedAlbums` accepts:

- `limit`: maximum number of albums in one response.
- `offset`: zero-based cache offset; omitted wire values default to zero for compatibility.
- `provider`: optional provider identity.

The daemon returns `ResponseData::MediaItems`. A response shorter than the requested page limit terminates pagination. The cache ordering is stable: descending `library_items.added_at_ms`, then case-insensitive album name.

### `FollowedArtists`

`Request::FollowedArtists` has the same `limit`, `offset`, and optional `provider` contract. Despite its legacy request name, its result is the **library artist index**:

1. explicitly followed artists;
2. unique artist credits from saved albums.

The second source prevents an unusable Artists view when Spotify's `/me/following` endpoint is unavailable or has not completed a sync. Explicit follows remain included and deduplicated by resource URI. Results are sorted case-insensitively by artist name before the requested page is selected.

The legacy name remains for IPC compatibility; new prose should call this the library artist index, not an exact mirror of Spotify follows.

## Cache implementation

`CacheStore::list_saved_albums(limit, offset, provider)` performs the database pagination using `LIMIT ? OFFSET ?`; it does not load the full saved-album collection into the daemon.

`CacheStore::list_saved_album_artists(limit, provider)` reads `media_items.artists_json` for saved album records, decodes `ArtistRef` values, removes duplicate URIs, and constructs provider-neutral `MediaItem { kind: Artist }` records. The daemon merges these with explicit followed artist rows, sorts, deduplicates, and applies the request offset/limit.

All provider predicates use the persisted `media_items.provider` identity. This is important: URI scheme alone is not a provider boundary.

## Client behavior

Use a page size of 50 for scrolling views. Keep one request in flight per collection. Append only the next contiguous page; refresh clears the local page cache and starts at offset zero. A short page marks the collection exhausted.

The desktop client tracks a `ScrollHandle` for Albums and Artists and requests another page when the final visible item is within ten rows of the loaded end. This bounded rendering is the primary scroll-jank mitigation: it avoids constructing thousands of GPUI elements and cover-art requests in one frame.

## CLI

The canonical non-desktop surface is:

```sh
spotuify library albums --limit 50 --offset 0 --format json
spotuify library artists --limit 100 --offset 0 --format json
```

Callers can advance `offset` by the number of returned rows. JSON output remains the stable pipeable interface.

## Failure behavior

These reads are cache-backed. They continue to work while upstream Spotify reads are rate-limited. If no cache data exists, the daemon's existing provider capability checks and bounded live fallback apply. Pagination does not create additional unbounded provider work.
