# Phase 6 - Sync Hardening

## Goal

Make sync correct under Spotify's rate limits, schema drift, and concurrent edits. Make the daemon's playback truth come from librespot's event stream, not polling. Add freshness signals and two-stage mutation receipts so optimistic UI is possible.

## Evidence base

Cross-checked against ncspot, spotify-player, and spotatui (all current as of May 2026):

- **None** of the three handle rate limits correctly. spotify-player has zero handling. ncspot does `thread::sleep` on Retry-After in the request path (freezes the UI). spotatui has a two-tier system where the raw-reqwest path respects Retry-After but the rspotify-direct path just string-matches `"429"` and shows a toast.
- **None** use ETag / If-None-Match. snapshot_id is the only durable concurrency token Spotify offers reliably; spotify-player only uses it for `reorder_playlist_items`; **ncspot uses it as a refetch gate during library sync** — `if local.snapshot_id == remote.snapshot_id { skip refetch }`. That's the pattern to copy.
- **All three** crash or silently fail when Spotify drops fields from response payloads. **spotatui** built a "compat normalizer" that walks the JSON and backfills missing keys (`available_markets`, `external_ids`, `linked_from`, `popularity`) before deserialization. That's the pattern to copy.
- **All three** poll the Web API after every state-changing action (spotify-player does 5 polls at 1-second intervals). With embedded librespot (Phase 9) the `PlayerEvent` stream is the real source of truth — polling becomes a fallback only.
- **spotatui** discovered a token-refresh bug (PR #217): if Spotify's refresh response omits `refresh_token`, you must merge the old one before writing or you re-auth on every restart.

## Deliverables

### Rate-limit middleware
- Single `RateLimitedClient` wrapping reqwest at the lowest seam in `crates/spotuify-spotify`.
- All Web API calls flow through it — no bypass for "raw" paths.
- Token-bucket budget per (scope, endpoint-class) with persistent state across daemon restarts.
- On 429: parse `Retry-After` (seconds or HTTP-date), clamp to a reasonable ceiling (60s default; configurable), tokio-sleep, retry once, then surface `SpotifyError::RateLimited { retry_after, scope }`.
- On 401: refresh token, retry once, surface `SpotifyError::AuthExpired` on second failure.
- Jittered exponential backoff (factor 2.0, jitter ±25%) on transient 5xx, max 3 attempts.
- Concurrency cap on background sync (default 2 in-flight); playback control bypasses the cap.

### Spotify response compat layer
- `compat_normalize(value: &mut serde_json::Value, hint: NormalizeHint)` runs before deserialization for known-drift endpoints.
- Hints per-shape: `Track`, `Album`, `Artist`, `Playlist`, `Episode`, `Show`, `Paging<Track>`, etc.
- Backfills missing keys with safe defaults (`available_markets: []`, `external_ids: {}`, `linked_from: null`, `popularity: 0`, etc.).
- Telemetry: emit `analytics_events.kind = "spotify_payload_compat"` with the missing-key set so we know what Spotify changed.
- Reference implementation: spotatui `src/infra/network/requests.rs:129-240`.

### snapshot_id and freshness
- `playlists` table gets `snapshot_id TEXT` column.
- Playlist sync flow: `GET /playlists/{id}` returns `snapshot_id`; compare to local; if equal, skip the expensive `GET /playlists/{id}/tracks` call.
- `playlist_items.snapshot_id_at_fetch` records the snapshot under which items were last fetched — used for stale-item detection.
- All cacheable rows gain:
  - `fetched_at_ms INTEGER`
  - `freshness_class TEXT` — `fresh` | `stale_but_usable` | `refreshing` | `failed_refresh` | `unknown`
  - `sync_generation INTEGER` — daemon bumps on each full sync to detect cache-version skew
- `saved_tracks` unchanged shortcut (ncspot pattern): if page-0 `(ids, total)` matches local, skip refetch.
- `cache_version INTEGER NOT NULL` constant in `spotuify-store`; bump on incompatible schema changes; daemon refuses to start with mismatch and surfaces a `spotuify cache reset --confirm` path.

### Two-stage mutation receipts
- `Receipt` becomes: `{ receipt_id: ReceiptId, action: String, status: Pending | Confirmed | Failed, message: String, started_at_ms, finished_at_ms: Option, error: Option<ApiError> }`.
- `Pending` receipts persist to SQLite so they survive a daemon crash.
- On `MutationAccepted`: daemon writes optimistic row delta + emits event with status `pending`.
- On Spotify success: row reconciled, receipt → `confirmed`, `MutationFinished` event.
- On Spotify failure: optimistic delta rolled back, receipt → `failed` with typed error, `MutationFinished` event with status.
- TUI binds spinner state to receipt status; CLI prints initial pending line, updates on completion (or `--wait-confirmed` flag for scripted use).

### PlayerEvent stream as truth
- With Phase 9's embedded librespot, daemon subscribes to `PlayerEvent::{Playing, Paused, Stopped, EndOfTrack, TrackChanged, Seeked, VolumeChanged, PositionChanged, SessionDisconnected}`.
- `DaemonEvent::PlaybackChanged` emits derived from `PlayerEvent`, not from polling.
- Web API `GET /me/player` polling is fallback-only: triggered on session disconnect, on librespot device-not-current, or on a 60s heartbeat if no `PlayerEvent` has arrived.
- Drop spotify-player's "5x retrieve-after-action" pattern entirely.

### Token refresh hardening
- After every token refresh, if response lacks `refresh_token`, merge the existing `refresh_token` from cache before writing.
- Persist credentials via the per-platform keyring plus a mode-0600 disk mirror. The mirror is intentional: it prevents detached daemons from hanging behind OS keychain prompts.
- Refresh proactively at `expires_at - 60s`, not on 401. 401 handling stays as a safety net.
- Token-refresh race: an in-process `tokio::sync::Mutex<TokenState>` serializes callers inside one daemon, and `<data_dir>/auth/token.lock` serializes daemon/CLI processes. Under the file lock, reload persisted credentials before refreshing stale memory.

### New daemon events
- `RateLimited { retry_after_secs, scope }`
- `AuthError { kind: ExpiredRefresh | InvalidGrant | Forbidden }`
- `MutationAccepted { receipt_id, action }`
- `MutationFinished { receipt_id, status, message }`
- `SchemaCompat { endpoint, missing_keys }` — telemetry-grade, helps us notice Spotify drift fast

### Typed error model
- `SpotifyError` enum replacing `anyhow::Error` at the spotify crate boundary:
  - `RateLimited { retry_after: Duration, scope: String }`
  - `AuthExpired`
  - `AuthRevoked`
  - `Forbidden { scope: String }`
  - `NotFound`
  - `Deprecated { endpoint: &'static str }`  — for endpoints retired Nov 2024
  - `Network(reqwest::Error)`
  - `Decode { endpoint: String, source: serde_json::Error }`
  - `Api { status: u16, message: String, body: String }`
- CLI maps to stable exit codes per `tests/cli_exit_codes.rs`.
- TUI shows differentiated banners (rate limit → countdown chip, auth expired → re-auth prompt, deprecated → "Spotify removed this; using local fallback if available").

## Work items

- [x] Build `RateLimitedClient` in `crates/spotuify-spotify/src/rate_limit.rs`. Test against fake provider that injects 429 / Retry-After. Verified with wiremock coverage for 429 retry, bounded sustained 429, and transient 5xx retry.
- [x] Add `SpotifyError` enum; convert call sites to return typed errors. Remove `anyhow` from the public surface of `spotuify-spotify`. Verified by `SpotifyResult` on the public client/action/selection/auth/config surfaces, `rg -n "pub (async )?fn .*-> Result|pub .*-> anyhow::Result|use anyhow::Result|anyhow::Result" crates/spotuify-spotify/src` finding no public surface hits, `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-spotify --quiet`, `CARGO_TARGET_DIR=target-cli cargo clippy -p spotuify-spotify --all-targets -- -D warnings`, and downstream `cargo check` for CLI/daemon/sync/TUI.
- [x] Add compat normalizer; wire to all paging/track/album/playlist endpoints; instrument with `SchemaCompat` events. Verified by `compat_normalize`, decode-path normalization in `SpotifyClient::request_json`, daemon `SchemaCompatReporter` wiring, `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-spotify compat_wiring --quiet`, `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-spotify --test compat_normalizer --quiet`, and `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-daemon schema_compat_reporter --quiet`.
- [x] Migrate `playlists` table to add `snapshot_id`; switch playlist sync to gated refetch. Verified by store migration tests for `playlists.snapshot_id` / `playlist_items.snapshot_id_at_fetch`, sync gate unit tests, and `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-sync --test refetch_gate --quiet`; the integration test covers cold-start fetch then unchanged-snapshot skip.
- [x] Add freshness columns to all cache tables; populate on writes; surface in `cache status`.
- [x] Implement saved-tracks unchanged shortcut. Verified by `saved_tracks_fingerprint_preserves_sync_order`, `library_sync_skips_saved_tracks_when_page_zero_is_unchanged`, and `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-sync --test refetch_gate --quiet`.
- [x] Add `cache_version` constant + startup gate. Verified by `Store::check_cache_version`, daemon startup guard in `DaemonState::new`, and `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-store test_check_cache_version --quiet`.
- [x] Add two-stage receipt machinery; persist pending receipts to a new `receipts` table; recover/finalize on daemon restart. Verified by `crates/spotuify-store/tests/receipts.rs`, `record_mutation_with_id` in the daemon handler, startup recovery in `DaemonState::new`, and `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-daemon receipt_recovery --quiet`.
- [x] Migrate playback state to `PlayerEvent`-driven model; keep polling as a fallback path. Verified by daemon PlayerEvent translation tests: playback start/pause/resume/track/end now emit `PlaybackChanged`, while high-frequency ticks remain local and existing polling stays as fallback.
- [x] Harden token refresh with `refresh_token` merge + proactive refresh + serialized refresh. Verified by `auth::access_token_cached` single-flight mutex, cross-process token-store lock tests, refresh-token merge tests, `refresh_planner` tests, and focused `spotuify-spotify` auth tests.
- [x] Add new daemon events. Verified protocol variants for `RateLimited`, `AuthError`, `MutationAccepted`, `MutationFinalized`, and `SchemaCompat`, daemon emitters for mutation/schema events, event-log lifting, and focused daemon/protocol tests.
- [x] Update TUI to render pending receipts with spinner and to bind banner state to `RateLimited`/`AuthError`/`Deprecated`. Verified by pending-receipt/banner state tests, status-line rendering, `CARGO_TARGET_DIR=target-cli cargo test -p spotuify-tui --quiet`, and TUI clippy.
- [x] Add `spotuify cache reset --confirm` + `spotuify cache repair` for operator escape hatches. Verified by `tests/cli_cache.rs`; reset is local so it still works when daemon startup is blocked by cache schema mismatch.

## Verification

- Fake provider injects 429 with `Retry-After: 2` on every 5th request → daemon completes sync without errors, emits two `RateLimited` events, total wall time matches the imposed delay.
- Inject a Feb-2026-style payload missing `available_markets`/`external_ids` → request succeeds, daemon emits `SchemaCompat` event listing the missing keys.
- Playlist sync with unchanged `snapshot_id` for 50 playlists → zero `/playlists/{id}/tracks` calls; `cache status` shows all rows `fresh`.
- `playlist add LIST URI` returns receipt with `status=pending` in <50ms; subsequent `MutationFinished` event flips to `confirmed`; if injected 5xx, status flips to `failed` and the optimistic row is gone from the DB.
- Embedded librespot `PlayerEvent::Playing` → `DaemonEvent::PlaybackChanged` arrives in <100ms.
- Kill network mid-mutation → receipt `failed`, optimistic delta reverted, `cache status` table shows `failed_refresh`.
- Restart daemon with a `cache_version` mismatch → daemon refuses to start, suggests `spotuify cache reset --confirm`.
- Refresh path: simulate Spotify omitting `refresh_token` from a refresh response → next restart still has a valid refresh token; no re-auth prompt.

## Definition of done

The daemon survives a multi-hour active session against the real Spotify API without rate-limit errors, ghost mutations, or stale caches. CLI receipts show pending → confirmed transitions. The TUI no longer "freezes" during bulk mutations. spotuify's behavior degrades gracefully when Spotify changes a payload shape and the user gets a clear banner explaining what's degraded and why.
