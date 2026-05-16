# Phase 3 - Local Store and Search

## Goal

Add SQLite cache and Tantivy search so local library/playlists/search history are fast and scriptable.

## Deliverables

- SQLite migrations.
- Store module with explicit SQL.
- Sync jobs for devices/playback/playlists/recent/library.
- Remote search result cache.
- Tantivy schema and indexing.
- `spotuify search --source local|spotify|hybrid`.
- `spotuify reindex`.
- `spotuify cache status`.

## Implementation order

1. [x] Add SQLite connection and migrations.
2. [x] Persist playback/device snapshots.
3. [x] Persist playlists and playlist items.
4. [x] Persist recent tracks and search results.
5. [x] Add local query over SQLite only.
6. [x] Add Tantivy index from SQLite.
7. [x] Add reindex command.
8. [x] Add background sync scheduler.

## Search schema starter

Fields:

- URI
- kind
- name
- artist names
- album name
- playlist name
- owner
- source
- liked/saved flags
- added timestamp
- duration

## Verification

- sync creates rows
- reindex creates documents
- local search works without Spotify network
- remote search caches results
- cache status shows row counts and freshness
- Store migration and operation tests cover SQLite schema/persistence.
- `spotuify-search` tests cover Tantivy indexing and stale-document replacement.
- CLI help/parser tests cover `search --source`, `reindex`, and `cache status`.

## Definition of done

Common library and playlist searches respond from local state, with Spotify refresh happening in background.
