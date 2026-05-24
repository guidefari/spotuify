# Idiomatic Rust: Deferred Backlog

> Findings from the 2026-05-23 idiomatic audit that were **intentionally not changed** in that pass,
> with the reason each was deferred. The driving constraint was "do not break the app": every item
> here either lacks a safety-net behavior test that can be run before/after in the current
> environment, or carries a blast radius that needs an incremental, separately-reviewed change.
> Items already fixed in the pass are listed at the bottom.

Judged against [`idiomatic-rust-rubric.md`](./idiomatic-rust-rubric.md). Rubric dimension in brackets.

## A. Async: blocking work on the Tokio runtime [D1]

1. **`spotuify-spotify/src/auth.rs:267,290,397`** — `acquire_token_store_lock_bounded` runs a blocking `fs2` file-lock poll loop with `std::thread::sleep`, and `load_token_bounded` reads the macOS Keychain, both on a runtime worker inside the token-refresh path while the `cache.lock().await` tokio mutex is held. **Fix:** move the blocking lock + keychain read into `tokio::task::spawn_blocking`. **Deferred because:** this is the hottest cross-cutting path (every Spotify call refreshes through it); there is no fake token-store seam to TDD it, and exercising it hits the real Keychain, which risks the prompt-storm documented in the `project_dev_build_keychain` memory. Needs a fake token-store/keychain seam first.

2. **`spotuify-system/src/cover_cache.rs:228,244`** — `image::load_from_memory` (CPU) and `std::fs::write`/`rename` (blocking IO) run inside `async fn fetch_and_persist_inner`. **Fix:** wrap decode + write in `tokio::task::spawn_blocking` (or `tokio::fs`). **Deferred because:** only reached on a cache miss, and there is no `get_or_fetch` behavior test; add a wiremock-backed test asserting fetched bytes + on-disk cache before refactoring.

3. **`spotuify-search/src/lib.rs:86-99`** — the search actor calls tantivy `commit()` / `search` inline on its tokio task (CPU/IO-blocking). **Fix:** `spawn_blocking` the commit/search, or run the actor loop on a dedicated thread. **Deferred because:** the actor already isolates the blocking to a single task (mitigates worker starvation); lower priority, and the `IndexWriter` lifetime across `spawn_blocking` needs care.

## B. Aggregate timeout [D2]

4. **`spotuify-sync/src/sync_loop.rs:162-167`** — the slow loop (`Playlists`, `Library`) calls `sync_target` without an aggregate timeout (the fast loop uses `sync_target_with_backoff` → `PER_TARGET_TIMEOUT` = 10s). **Fix:** a *generous* `SLOW_TARGET_TIMEOUT`. **Deferred because:** reusing the 10s `PER_TARGET_TIMEOUT` would truncate legitimate large-library paginated syncs (a real regression). Each underlying HTTP call is already bounded by the 8s reqwest timeout, so no infinite hang is possible; a correct aggregate bound must be sized against worst-case library size and validated, ideally with a fake `SyncContext` test (see H4).

## C. Structural maintainability [F1, F2]

5. **`spotuify-daemon/src/handler.rs:64`** — `dispatch` is a 1190-line, 62-arm god-function in the integration crate. **Fix:** extract each fat arm into `async fn handle_<request>(state, …)`, leave `dispatch` a thin router. **Deferred because:** highest comprehension/merge-conflict cost, but it is the daemon's hottest function; extraction must be incremental with per-request behavior tests to avoid breakage.

6. **`spotuify-tui/src/app.rs:221`** — `App` has 79 `pub` fields mixing client view-state with daemon-owned render caches. **Fix:** group into `ViewState`/`ModalState`/`DaemonCache` sub-structs; demote `pub`→`pub(crate)`/private. **Deferred because:** the TUI is human-verified only (no render/golden tests), so a wide refactor across `app.rs`/`ui.rs` has no automated safety net.

7. **`spotuify-store/src/lib.rs`** — raw `sqlx::query("...")` strings throughout; no compile-time schema check [G3]. **Fix:** `sqlx::query!`/`query_as!` with a committed offline `.sqlx` cache. **Deferred because:** highest compile-time-safety payoff but the widest, most schema-coupled surface; needs offline-cache setup against a live schema plus per-query migration.

8. **God-file splits [F2]:** `protocol/lib.rs` (1883), `spotify/client.rs` (2912), `store/lib.rs` (3180), `tui/ui.rs` (4937). **Fix:** split by concern into submodules. Mechanical but large; pair with the items above.

## D. Stringly-typed round-tripping [B2, C4]

9. `label()` + `from_label()`/`parse()` enum pairs across `core`/`protocol`/`store` (`MediaKind`, `LyricsProvider`, `OperationSource`, `parse_kind`/`parse_status`, `media_kind_from_label`) duplicate `#[serde(rename_all)]` and hand-roll reverse maps. **Fix:** `Display`/`FromStr` (or `TryFrom<&str>`) on the defining enum; store/protocol reuse it. **Deferred because:** touches many enums + call sites; needs a round-trip test per enum (`label()` ↔ `parse()` ↔ serde) first.

10. `protocol/lib.rs:607 classify_error_kind`, `event_log.rs:61 format!("{:?}", kind)`, `store retry_after_seconds`, `search` schema-mismatch `contains("schema does not match")` — error *kind* / control flow recovered by substring-matching `Display`/`Debug` text. **Fix:** carry the typed kind structurally across the IPC seam. **Deferred because:** threading typed kinds through the wire contract is a protocol change needing its own tests.

## E. Low-value polish (recommendations, not planned work)

- Per-byte hex `format!("{:02x}")` → `hex::encode`/`write!` in `core::sha256_hex` and `player::derive_device_id`. Skipped: micro-allocation in cold paths; `hex` would add a dependency to the foundational `core` crate.
- `#[must_use]` on pure predicates (`should_refetch_*`, `PrivacyGate::is_private`). Skipped: `must_use_candidate` is in the rubric allow-list (noisy); add selectively if wanted.
- Stale `spotuify-player/src/lib.rs:1-17` module doc still describes the removed Spotifyd/ConnectOnly backends [I1]. Safe doc-only fix; do opportunistically.
- `premium_gate.rs:79 .expect()` on `Client::builder().build()` → return `GateError` [C1].
- `cli/commands.rs:30 ipc_search` 8-arg `#[allow(too_many_arguments)]` → `SearchArgs` struct [E2].
- `store migrations.rs:56` tautological `CACHE_VERSION == 12` → an invariant like `CACHE_VERSION as usize == MIGRATIONS.len()` [H3]. Verify the true relationship (`.len()` vs `.last().version`) before changing the assert.

---

## Fixed in the 2026-05-23 pass (TDD: green → refactor → green)

- `protocol/event_log.rs` — bounded FIFO `Vec` + `remove(0)` (O(n) shift) → `VecDeque` + `pop_front` (O(1)). Covered by `event_log_drops_oldest_when_over_capacity` (asserts eviction order; survives the swap). [F4, D-perf]
- `keychain/Cargo.toml` — `thiserror = "1"` → `{ workspace = true }` (v2); removes a duplicate-major direct dependency. [G2]
- `search/lib.rs:320` — removed a discarded `Count` collector query that ran full-result counting on every search and threw the result away. [F3, perf]
- `spotify/rate_limit.rs:204,211` — `&PathBuf` → `&Path` on `BackoffState::load`/`save` (clippy `ptr_arg`; accepts more callers). [F4]
- `daemon/server.rs:155` — accept-loop `accepted?` (a transient accept error killed the whole daemon and skipped graceful drain) → log + brief backoff + continue. [D5, robustness]
