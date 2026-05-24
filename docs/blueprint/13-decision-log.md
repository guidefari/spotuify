# spotuify - Decision Log

This document records settled decisions so future agents do not re-litigate them without new evidence.

## D001: Architecture - daemon-backed, not TUI-owned

Chosen: daemon-backed runtime with CLI/TUI clients.

Considered:

- monolithic TUI that calls Spotify directly
- CLI-only controller
- daemon-backed runtime

Why:

- music must continue after TUI exits
- CLI and agents need the same capabilities
- local cache/search requires background work
- TUI state should not be durable app state

## D002: CLI is canonical

Chosen: CLI-first product surface.

Why:

- every action becomes testable
- agents can use the app safely
- scripts and pipelines become first-class
- TUI-only features are hard to verify and easy to break

## D003: Playback device - use Spotify Connect, not Web API audio

Chosen: controller plus Spotify Connect device.

Why:

- Spotify Web API does not stream audio
- spotifyd/librespot or official apps are the playback devices
- spotuify should control, not impersonate a streaming client unless we deliberately embed librespot later

## D004: Search - local first, Spotify remote as provider

Chosen: SQLite/Tantivy local search plus live Spotify search.

Why:

- saved library and playlist data should be instant
- remote API is rate-limited and occasionally flaky
- agents need repeatable search results

## D005: Output formats are stable product contract

Chosen: table/json/jsonl/csv/ids on data commands.

Why:

- Unix composition
- agent integration
- testability
- less screen scraping

## D006: Lyrics are optional provider, not core Spotify feature

Chosen: no core lyrics promise until a legal/provider-backed source exists.

Why:

- Spotify Web API does not expose official lyrics
- unreliable scraping would make the player feel broken

## D007: TUI UX follows contextual action registry

Chosen: action registry drives hint bar, command palette, help, and command availability.

Why:

- prevents hidden keymap mystery
- keeps hints contextual
- makes CLI/TUI parity auditable
- supports future configurable keymaps

## D008: Implementation strategy - copy mxr before inventing

Chosen: copy/adapt mxr implementations for daemon, IPC, SQLite, Tantivy, CLI output, mutation discipline, and TUI async/action plumbing wherever the shape matches.

Considered:

- greenfield spotuify-specific architecture
- copying mxr first, then extracting shared crates after repetition is proven
- extracting shared crates before spotuify uses the patterns

Why copy first:

- mxr has already paid the design/debugging cost for local daemon architecture
- daemon/IPC/store/search mechanics are nearly identical across these terminal-native apps
- copy/paste/adapt is faster and safer than designing abstractions too early
- after two or three apps share the same shape, extraction targets become obvious

Future extraction candidates:

- local JSON IPC codec/client/server
- daemon lifecycle and socket management
- CLI output rendering formats
- mutation preview/confirmation/receipt helpers
- TUI action registry, keymaps, hint bar, command palette
- SQLite/Tantivy sync/index scaffolding

Do not abstract before the second real use case proves the seam.

## D009: TUI-only actions must stay client-scoped

Chosen: actions that touch Spotify, cache, search, playlist, queue, device, or daemon state need a CLI equivalent. TUI-only actions are allowed only for client-local navigation, discovery, input, selection, and layout state.

Current TUI-only actions:

- `Command Palette` - client discovery surface
- `Help` - client help overlay
- `Quit TUI` - closes the TUI client only
- `Move Down` - client navigation state
- `Move Up` - client navigation state
- `Page Down` - client navigation state
- `Page Up` - client navigation state
- `Jump Top` - client navigation state
- `Jump Bottom` - client navigation state
- `Back` - client navigation state
- `Filter Current List` - client-side visible-list filter
- `Cancel Input` - client text input state
- `Mark Item` - client multi-select state
- `Mark Range` - client multi-select state
- `Clear Marks` - client multi-select state
- `Toggle Player Size` - client layout preference
- `Expand Rail` - client layout preference
- `Devices` (quick-pick overlay) - client overlay shortcut

Why:

- these actions do not mutate reusable app state
- daemon IPC should not expose screen cursor, modal, hint, or layout state
- CLI parity remains mandatory for reusable music capabilities

## D010: Embedded librespot (Phase 9, decision gate)

Chosen: embed librespot in the daemon behind a `--features embedded-playback` cargo feature; keep `--backend spotifyd` supported for users who want crash isolation.

Why:

- All three active Rust Spotify TUIs (ncspot, spotify-player, spotatui) embed librespot 0.8.x; the install story improves from "install + configure spotifyd separately" to a single binary
- Sub-100ms playback control via direct `Spirc`/`Player` API instead of multi-second Web API roundtrips
- librespot's `PlayerEvent` stream replaces 60s polling for playback truth (per Phase 6)
- Mercury bus access unlocks lyrics + radio + related-artists endpoints Spotify killed in November 2024

Trade-offs accepted:

- Cargo tree grows ~30-40%, binary size from a few MB to ~25-40MB
- Audio-backend bugs come in-house (CoreAudio quirks on mac, PipeWire/PulseAudio selection on linux)
- librespot protocol drift maintenance now ours rather than spotifyd's release cycle
- Mitigated by spatatui's `RecoveringSink` pattern wrapping the backend Sink in `catch_unwind`

Implementation lands in Phase 9; not part of the current Phase 6/7/8 batch.

Implementation status (Phase 9.0–9.5 complete, 2026-05-13):

- `PlayerBackend` trait + 5 new typed `DaemonEvent`s (`PlayerReady`,
  `PlayerDegraded`, `PremiumRequired`, `SessionDisconnected`,
  `PlayerFailed`) in `crates/spotuify-player` and
  `crates/spotuify-protocol`.
- Backends shipped: `ConnectOnlyBackend` (Web API only, Free-tier
  capable, wiremock-tested), `SpotifydBackend` (preserves today's
  default), `MockPlayerBackend` (behind `test-support` feature),
  `EmbeddedBackend` with librespot 0.8 cache wiring, attachable sink
  chain, Player + Spirc registration, transport forwarding, and
  librespot event translation. Live Spotify Premium smoke is still
  required before flipping the default.
- Foundations for Phase 9.3 — `RecoveringSink` (catch_unwind with
  rolling panic budget), `Clock` trait + position-as-SystemTime
  derivation (NTP-step safe), worker `tokio::select!` loop
  (interval ticks only when playing) — all unit-tested.
- Foundations for Phase 9.4 — `MercuryFetcher` trait + TTL cache,
  `TokenBridge` (5s timeout, graceful refresh fallback) — both
  unit-tested.
- Audio backend matrix: `alsa-backend`, `pipewire-backend`,
  `rodio-backend`, `portaudio-backend` Cargo features; `compile_error!`
  guard when `embedded-playback` is enabled without one selected.
  Linux pulse env vars set on `EmbeddedBackend::new`.
- vergen pin deviation: the planning doc called for
  `vergen=9.0.6 + vergen-lib=9.1.0 + vergen-gitcl=1.0.8`. In practice
  vergen 9.0.6 is the right pin because vergen-gitcl 1.0.x is
  internally on vergen-lib 0.1.x; mixing in 9.1.x of vergen-lib
  produces two coexisting versions and breaks `librespot-core`'s
  build script. Comment lives in the workspace `Cargo.toml`.

## D011: MCP server as a first-class spotuify surface (Phase 8)

Chosen: ship `spotuify-mcp` as a workspace crate and a separate binary, exposing the daemon's Request set as Model Context Protocol tools and resources over stdio (default) or HTTP.

Why:

- No prominent Rust-native Spotify MCP exists in 2026; the Python servers (varunneal, tylerpina, Carrieukie) are Web-API-only with no local cache, no librespot playback, no analytics
- The daemon already speaks length-delimited JSON over Unix socket with typed Request/Response/Event; exposing the same types as MCP tools is incremental
- LLM clients (Claude Code, Cursor, Continue) can consume spotuify as a tool without shelling out
- Mercury-bus tools (lyrics/radio/related-artists, Phase 9 gated) and analytics tools (Phase 10 gated) give MCP clients capabilities the Python servers can't match

Discipline:

- Destructive tools (`playlist_create`, `playlist_add`, `library_save`, etc.) require explicit `confirm: true` in args. Without it the bridge returns a preview. Mirrors spotify-player commit #966 at the MCP layer.
- `undo_last` bypasses confirm -- it IS the safety net.
- Tools deferred to later phases surface a clear `LocalDeferred` marker rather than silently failing.

Pure-function core (tool catalogue, confirm gating, request bridge) tested with 31 unit tests; insta golden manifest snapshot locks the public tool surface so additions/renames are always a code-review event. The rmcp wire integration (stdio + HTTP transport) lands as a follow-up on top of the same core.

## D013: HealthClass has three variants (Phase 13)

Chosen: `HealthClass { Healthy, Degraded, Unhealthy }`.

Considered:

- two variants (Healthy/Degraded only)
- three variants (Healthy/Degraded/Unhealthy)
- four variants (mirroring mxr's `Healthy/Degraded/RestartRequired/RepairRequired`)

Why three:

- Two variants conflated "running with a soft failure" with "cannot reach Spotify at all". Operators and monitoring scripts need to act differently on those.
- Four variants over-fit the email-client domain (mxr); spotuify's recovery path is `daemon restart` or `login` re-auth in either case, so RestartRequired vs RepairRequired didn't pay rent.
- Doctor election is now: any `Error` finding → Unhealthy, any `Warning` → Degraded, else Healthy.

Implementation lands in `crates/spotuify-protocol/src/lib.rs` (enum) plus `crates/spotuify-daemon/src/diagnostics.rs:finalize_report` (election).

## D014: Competitor study citation (Phase 13)

Chosen: record the open-source Rust Spotify TUIs/MCP servers we studied and the patterns adopted from each. The blueprint cribbed liberally; this entry locks the provenance.

Sources studied (2025–2026):

- `ncspot` — cursive-based TUI; lifted: per-playlist `snapshot_id` as concurrency token (`model/playlist.rs:25`), MPRIS via direct zbus (`src/mpris.rs`), `panic.rs` terminal-restoration hook, `reload` and `reconnect` commands (`commands.rs:213-235`, `application.rs:275-284`).
- `spotify-player` — ratatui TUI + Connect API client; lifted: souvlaki media-controls + hidden-window pattern (`src/media_control.rs:160-263`), shell `player_event_hook_command` (`src/streaming.rs`), `-o key.path=value` config override (`config/mod.rs:526-553`), confirmation popups on destructive actions (commit #966 → Phase 13's TUI modal + Phase 8 MCP confirm gate).
- `spotatui` — Connect + analytics TUI; lifted: auto-`.gitignore` in config dir (`core/config.rs:99-115`), `RecoveringSink` (catch_unwind panic budget for librespot, Phase 9.3), Discord Rich Presence pattern (`infra/discord_rpc.rs`), macOS NowPlaying scaffolding (`infra/macos_media.rs`).
- `mxr` (planetaryescape) — email client; lifted: file-polling `logs tail --follow` loop (`crates/daemon/src/commands/logs.rs:48-142`), `bug-report` assembly + redaction (`crates/daemon/src/commands/bug_report.rs:57-216`), clap-built-in `generate completions` (`crates/daemon/src/commands/completions.rs`), JSON-to-file + text-to-stdout tracing layering pattern (`crates/daemon/src/lib.rs:965-1006`), undo-window snapshot/restore pattern (`crates/store/src/undo.rs`, adapted in spotuify-daemon/src/undo.rs).
- `jj` (mercurial-style VCS) — adopted `op log` + `op undo` model whole. The DAG-of-views richness was not adopted; spotuify uses a linear op log with `subject_op_id` linkage so the schema stays SQLite-friendly.

Date recorded: 2026-05-14.

## D012: Operation log + undo (Phase 12)

Chosen: every daemon mutation records an `operations` row with a reversal plan, surfaced via `spotuify ops log` / `spotuify ops undo` and the MCP `undo_last` tool.

Why:

- Phase 8 lets LLMs mutate state; without undo, a misfired tool call is unrecoverable without manual SQL or Spotify-app intervention
- jj's `op log` + `op undo` pattern is the established 2026 shape for "I let an agent do things and want a back button"
- Phase 6's two-stage receipts already capture mutation intent; the operations table extends it with persistent reversal plans plus snapshot_id concurrency tokens for safe rollback

Implementation lands in Phase 12; not part of the current Phase 6/7/8 batch.

## D015: First-party (keymaster) Web API auth (2026-05-24)

Chosen: drop the per-user Spotify Developer app as the default. spotuify
logs in with librespot's first-party "keymaster" client id
(`65b708073fc0480ea92a077233ca87bd`) via `librespot-oauth`, and mints the
Web API bearer from the live librespot session with
`Session::login5().auth_token()`.

Why:

- Spotify put dev-mode apps behind a 5-user allow-list AND blocked
  playlist writes for them (Feb 2026). Verified 2026-05-24: a dev-app
  token gets `403 Forbidden` on `POST /users/{id}/playlists` and
  `POST /playlists/{id}/tracks`; the keymaster token gets `429`
  (authorized, only rate-limited). Allow-listing + re-login did not help.
- This is what every working terminal client does (spotify-player,
  ncspot). The keymaster client is never in Development Mode.
- It also deletes spotuify's worst onboarding step — there's no client_id
  to register/paste. One browser login and you're in.

How (as built):

- `login5().auth_token()` is the primary bearer source (full scope,
  re-mintable from the live session without a browser, survives
  keymaster-OAuth-endpoint outages). The raw `librespot-oauth` access
  token (refreshed via `refresh_token_async`) is the bootstrap +
  fallback — it's a valid full-scope bearer on its own (probe-proven).
- The bearer reaches the Web API client through a `WebApiBearerProvider`
  trait (`spotuify-spotify`), implemented in the daemon by minting via
  the player actor's `PlayerBackend::web_api_token()` (login5). The
  entire legacy dev-app PKCE path is left intact behind this seam.
- Persistence: only the librespot-oauth refresh token is stored
  (`FirstPartyCredentials`, keychain account `spotify-first-party` +
  0600 disk mirror). The bearer is never persisted; reusable native
  playback credentials live in librespot's own cache.
- Opt-out: set `SPOTUIFY_CLIENT_ID` (env) to use your own Spotify app
  (legacy dev-app flow). The opt-out is the **env var**, not a config
  client_id — the old onboarding wrote the user's dev-app id into the
  config, so keying off the config value would strand existing users on
  the broken flow. Env-only opt-out migrates everyone to the fix and
  lets the next launch send them through the browser login.
- Scope-drift banner is suppressed in first-party mode: login5 tokens
  always report empty scopes, so the check would fire a permanent false
  "run spotuify login".

Full staged plan: `docs/blueprint/auth-rework-plan.md`.
