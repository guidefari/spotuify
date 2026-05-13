# Phase 13 - Spec Compliance and QoL Cleanup

## Goal

Close small gaps between code and blueprint specs, plus adopt small-but-impactful QoL patterns the competitors all agree on.

## Evidence base

| Pattern | Source | Why |
|---|---|---|
| `reload` command for hot-config reload | ncspot `commands.rs:213-235` | Re-reads config, rebuilds theme, rebinds keys; no restart |
| `reconnect` command | ncspot `application.rs:275-284` | Rebuilds session after network change |
| `-o KEY=VALUE` config override on CLI | spotify-player `config/mod.rs:526-553` | Dot-path override of TOML values without editing file |
| Auto-generated `.gitignore` in config dir | spotatui `core/config.rs:99-115` | Hedge against dotfile-sync token leaks |
| `cache_version` constant for schema invalidation | ncspot `config.rs:17`, `library.rs:97-103` | Cache breaks safe and explicit |
| Confirmation popups on destructive actions | spotify-player commit #966 | TUI parity with Phase 8 MCP confirmation pattern |
| Token-refresh `refresh_token` merge | spotatui PR #217 | Prevents re-auth on every restart |
| `User-Agent` header on all outbound HTTP | spotatui | Etiquette for LRCLIB and Spotify operability |
| Backtrace dump on panic | ncspot `panic.rs`, spotify-player `main.rs:84-93` | Stdout is owned by TUI; logs need to go to file |

## Deliverables

### CLI flags and commands
- `spotuify sync search-cache --prune [--older-than 30d]` (blueprint `04-sync-cache.md`).
- `--no-daemon-start` global CLI flag (Phase 2 spec).
- `spotuify bug-report [--include-logs N]` — bundle redacted system info + logs + doctor report (blueprint `10-observability.md`).
- `spotuify reload` — daemon re-reads `config.toml`, rebuilds theme, rebinds keys; restarts player only if backend changed.
- `spotuify reconnect` — daemon rebuilds librespot session; useful after VPN/network change.
- `-o key.path=value` global CLI flag for one-shot TOML config override (e.g., `spotuify -o player.bitrate=160 play "jazz"`).
- `spotuify generate completions <shell>` (clap-built-in, just wire it).
- `spotuify generate man-page`.

### Config & UX patterns
- Auto-write `.gitignore` in `~/.config/spotuify/` listing `*.json`, `credentials.*`, `*.encrypted` on first config init.
- `cache_version` constant in `crates/spotuify-store`; daemon refuses to start with mismatch and suggests `spotuify cache reset --confirm`.
- `User-Agent: spotuify/<version> (https://github.com/bhekanik/spotuify)` on every outbound HTTP request (Spotify, LRCLIB, image downloads).
- Backtrace dump on panic to `~/.cache/spotuify/backtrace/<ts>.log` with terminal-restoration cleanup; surface "panic occurred — see logs" message on next start.
- Confirmation modals in TUI for destructive actions (delete playlist, unfollow user-followed playlist, bulk unsave). Mirrors Phase 8 MCP `confirm: true` discipline at the TUI layer.

### Observability
- `tracing-subscriber` JSON output mode via `--log-format json` or `SPOTUIFY_LOG_FORMAT=json`. Agent-consumable logs.
- `spotuify logs tail --follow --format json` streams structured events.
- Doctor reports: backend kind, audio backend, MPRIS bus name, image protocol, lyrics provider, MCP server state if running, cache version, last rate-limit event.

### HealthClass
- Promote `HealthClass` to three variants: `Healthy`, `Degraded`, `Unhealthy` (cannot reach Spotify, no auth, no daemon at all). Or document the two-variant choice in D013. Recommended: add the third variant.

### Decision-log backfill
- D010 — librespot embed (records Phase 9 outcome).
- D011 — MCP server (Phase 8 commitment).
- D012 — operation log (Phase 12 commitment).
- D013 — HealthClass cardinality.
- D014 — competitor study (this commit; record the source repos studied and date).

### README rewrite
- Match the shipped CLI surface; remove pre-daemon-era language.
- Add per-platform quickstart sections (filled by Phase 11).
- Add MCP-server setup snippet (Phase 8).
- Add embedded-vs-spotifyd choice (Phase 9).
- Add competitor comparison table.

### Phase 5 doc clarification
- Add a "Note on agent semantics" section to `06-phase-5-agent-playlists.md` clarifying that `build_playlist_plan` is a heuristic scaffold and that real LLM-driven planning lives in the upstream agent / MCP-client model.

## Work items

1. Add `sync search-cache --prune` subcommand.
2. Thread `--no-daemon-start` through clap root and CLI IPC wrappers; clear error if daemon not running and flag is set.
3. Implement `bug-report`: collect `doctor` JSON, last N log lines, `--version`, `cache status`, last 50 operations, redacted config; bundle as tarball; never auto-upload.
4. Implement `spotuify reload` request in daemon; live-rebuild theme + keymap; restart player only if backend changed.
5. Implement `spotuify reconnect` request; force `Session::new`.
6. Implement `-o key.path=value` global flag in clap; round-trip through TOML's `Value` tree (spotify-player's pattern).
7. Auto-write `.gitignore` in config dir on first init.
8. `cache_version` constant + startup gate (Phase 6 has the column; this phase adds the comparison).
9. `User-Agent` header in HTTP middleware.
10. Panic hook wiring + log path + cleanup-and-restart-message on next start.
11. TUI confirmation modals for destructive actions.
12. `tracing-subscriber` JSON formatter behind flag.
13. `logs tail --follow --format json`.
14. Promote `HealthClass` enum + doctor logic.
15. Decision-log entries D010-D014.
16. README rewrite.
17. Phase 5 doc clarification edit.

## Verification

- `spotuify --help` snapshot updates clean.
- `spotuify --no-daemon-start status` errors clearly when daemon not running; succeeds when it is.
- `spotuify bug-report` produces tarball; manual inspection shows no secrets.
- `spotuify -o player.bitrate=96 play X` plays at 96kbps for that invocation only; config file unchanged.
- `spotuify reload` after editing `config.toml` updates the running daemon without losing playback.
- `spotuify reconnect` after toggling VPN reconnects librespot session.
- `SPOTUIFY_LOG_FORMAT=json spotuify status` emits valid JSONL on stderr.
- `spotuify sync search-cache --prune --older-than 7d --format json` reports pruned counts.
- TUI delete-playlist action shows confirmation modal; `q` cancels, `y` confirms.
- Trigger panic (test-only `--panic-test` flag) → backtrace file written, terminal restored, next start surfaces the panic message and log path.
- Decision log matches actual decisions made in Phases 8, 9, 12, 13.

## Definition of done

Every CLI/event/setting promised in the blueprint or implementation plan is either implemented or has a referenced decision-log entry explaining the deliberate omission. The competitor-cribbed QoL patterns (`reload`, `reconnect`, `-o`, auto-gitignore, cache_version, confirmation modals, User-Agent, panic-to-file) are all in. Documentation reflects shipped reality. The blueprint stops drifting from the code.
