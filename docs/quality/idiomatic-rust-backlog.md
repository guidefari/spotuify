# Idiomatic Rust: Deferred Backlog

> Findings from the 2026-05-23 idiomatic audit that were **intentionally not changed** in that pass,
> with the reason each was deferred. The driving constraint was "do not break the app": every item
> here either lacks a safety-net behavior test that can be run before/after in the current
> environment, or carries a blast radius that needs an incremental, separately-reviewed change.
> Items already fixed in the pass are listed at the bottom.

Judged against [`idiomatic-rust-rubric.md`](./idiomatic-rust-rubric.md). Rubric dimension in brackets.

## A. Structural maintainability [F1, F2]

1. **`spotuify-daemon/src/handler.rs:64`** — `dispatch` is a 1190-line, 62-arm god-function in the integration crate. **Fix:** extract each fat arm into `async fn handle_<request>(state, …)`, leave `dispatch` a thin router. **Deferred because:** highest comprehension/merge-conflict cost, but it is the daemon's hottest function; extraction must be incremental with per-request behavior tests to avoid breakage.

2. **`spotuify-tui/src/app.rs:221`** — `App` has 79 `pub` fields mixing client view-state with daemon-owned render caches. **Fix:** group into `ViewState`/`ModalState`/`DaemonCache` sub-structs; demote `pub`→`pub(crate)`/private. **Deferred because:** the TUI is human-verified only (no render/golden tests), so a wide refactor across `app.rs`/`ui.rs` has no automated safety net.

3. **`spotuify-store/src/lib.rs`** — raw `sqlx::query("...")` strings throughout; no compile-time schema check [G3]. **Fix:** `sqlx::query!`/`query_as!` with a committed offline `.sqlx` cache. **Deferred because:** highest compile-time-safety payoff but the widest, most schema-coupled surface; needs offline-cache setup against a live schema plus per-query migration.

4. **God-file splits [F2]:** `protocol/lib.rs` (1943), `spotify/client.rs` (3054), `store/lib.rs` (3280), `tui/ui.rs` (5333), `tui/app.rs` (7748), `daemon/handler.rs` (5362). **Fix:** split by concern into submodules. Mechanical but large; pair with the items above.

## B. Low-value polish (recommendations, not planned work)

- Per-byte hex `format!("{:02x}")` → `hex::encode`/`write!` in `core::sha256_hex` and `player::derive_device_id`. Skipped: micro-allocation in cold paths; `hex` would add a dependency to the foundational `core` crate.
- `#[must_use]` on pure predicates (`should_refetch_*`, `PrivacyGate::is_private`). Skipped: `must_use_candidate` is in the rubric allow-list (noisy); add selectively if wanted.
- `premium_gate.rs:79 .expect()` on `Client::builder().build()` → return `GateError` [C1].
- `cli/commands.rs:30 ipc_search` 8-arg `#[allow(too_many_arguments)]` → `SearchArgs` struct [E2].

---

## Fixed (TDD: green → refactor → green)

- `protocol/event_log.rs` — bounded FIFO `Vec` + `remove(0)` (O(n) shift) → `VecDeque` + `pop_front` (O(1)). Covered by `event_log_drops_oldest_when_over_capacity` (asserts eviction order; survives the swap). [F4, D-perf]
- `search/lib.rs:320` — removed a discarded `Count` collector query that ran full-result counting on every search and threw the result away. [F3, perf]
- `spotify/rate_limit.rs:204,211` — `&PathBuf` → `&Path` on `BackoffState::load`/`save` (clippy `ptr_arg`; accepts more callers). [F4]
- `daemon/server.rs:155` — accept-loop `accepted?` (a transient accept error killed the whole daemon and skipped graceful drain) → log + brief backoff + continue. [D5, robustness]
- `spotify/auth.rs` — blocking file lock and auth-file IO moved into `spawn_blocking`; concurrent refresh behavior is covered by auth tests. [D1]
- `system/cover_cache.rs` — image decode and cache writes moved into `spawn_blocking`; cover cache fetch/write tests cover the path. [D1]
- `search/lib.rs` — Tantivy actor now runs on a blocking worker; schema mismatch recovery matches typed `TantivyError::SchemaError`. [D1, C4]
- `sync/sync_loop.rs` — slow playlist/library targets now have a 30-minute aggregate timeout with a focused test. [D2]
- `core`/`protocol`/`store` — `MediaKind`, `LyricsProvider`, `OperationSource`, `OperationKind`, and `OperationStatus` now implement `Display`/`FromStr`; store parsing reuses those traits with round-trip tests. [B2]
- `protocol`/`daemon`/`store` — IPC response errors are constructed with typed `IpcErrorKind`, sync rate-limit cooldown persists typed `retry_after_secs`, and legacy text parsing is retained only for pre-v13 rows. [C4]
- `store/tests/migrations.rs` — removed tautological `CACHE_VERSION == 12`; tests now assert applied migration count and max version match `CACHE_VERSION`. [H3]
