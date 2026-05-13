# Phase 8 - MCP Server

## Goal

Expose the daemon's Request/Event surface as a Model Context Protocol (MCP) server so LLM clients (Claude Code, Cursor, Continue, agent harnesses) can use spotuify as a first-class tool without shelling out to the CLI. Cover lyrics/radio/recommendations replacements via mercury bus (Phase 9). Adopt destructive-action confirmation patterns.

## Strategic rationale

- Existing Spotify MCP servers (varunneal, tylerpina, Carrieukie, iankan04) are all Python and Web-API-only. None have local cache, librespot playback, mercury bus access, or analytics.
- **No prominent Rust-native Spotify MCP exists.** Single largest 2026 differentiator the blueprint does not yet name.
- spotuify's daemon already speaks length-delimited JSON over a Unix socket with typed Request/Response/Event types. Exposing those as MCP tools is incremental, not a rewrite.
- Embedding librespot (Phase 9) unlocks endpoints the Web-API-only MCP servers can't offer: lyrics, radio, related-artists, recommendations — all post-Nov-2024 alive on mercury.

## Reference patterns

| Pattern | Source | Lesson |
|---|---|---|
| Confirmation popups on destructive actions | spotify-player commit #966 | Every destructive MCP tool should require explicit `confirm: true` argument |
| Mercury bus for lyrics | spotify-player `client/mod.rs:642-661` | `hm://lyrics/v1/track/{id}` |
| Mercury bus for radio | spotify-player `client/mod.rs:949-1019` | `hm://autoplay-enabled/query`, `hm://radio-apollo/v3/stations/` |
| `login5().auth_token()` | spotify-player `token.rs:8-46` | Avoid second OAuth flow for the MCP-side Web API client |

## Deliverables

- New crate `crates/spotuify-mcp` producing a `spotuify-mcp` binary.
- `spotuify mcp [--stdio | --http <addr>]` subcommand wired into `main.rs`.
- MCP tool definitions for every safe daemon Request, with JSON Schema.
- MCP resource definitions for playback state, devices, playlists as subscribable resources backed by `DaemonEvent`.
- Auto-spawn daemon if not running.
- Destructive operations gated by `confirm: true` in MCP tool args.
- README docs: Claude Code / Cursor / Continue config snippets.
- Decision-log entry D011.

## Tools to expose

| MCP tool | Backing | Notes |
|---|---|---|
| `search` | `Request::Search` | Default `source: hybrid`. JSON Schema includes `query`, `kind`, `limit`. |
| `now_playing` | `Request::PlaybackGet` | Track + device + progress + lyrics line if available |
| `play` | `Request::PlaybackCommand::PlayQuery` | First search hit |
| `play_uri` | `Request::PlaybackCommand::PlayUri` | Direct URI |
| `pause` / `resume` / `next` / `previous` | Transport | |
| `seek` / `volume` | Transport | |
| `shuffle` / `repeat` | Transport | |
| `queue_add` | `Request::QueueAdd` | URIs or query |
| `queue_show` | `Request::QueueGet` | |
| `devices_list` | `Request::DevicesList` | |
| `transfer_device` | `Request::DeviceTransfer` | Idempotent |
| `playlists_list` | `Request::PlaylistsList` | |
| `playlist_tracks` | `Request::PlaylistTracks` | |
| `playlist_create` | `Request::PlaylistCreate` | **Requires `confirm: true`** to commit; without it returns dry-run preview only |
| `playlist_add` | `Request::PlaylistAddItems` | **Requires `confirm: true`** |
| `playlist_remove` | `Request::PlaylistRemoveItems` | **Requires `confirm: true`** |
| `library_save` / `library_unsave` | `Request::LibrarySave` | **Requires `confirm: true`** |
| `lyrics` | Phase 16 lyrics provider | Returns synced lines + provider + offset |
| `radio_start` | Mercury `hm://autoplay-enabled/query` + `hm://radio-apollo/v3/stations/` | Replacement for dead `/recommendations`; starts a station from current track or specified URI |
| `related_artists` | Mercury `hm://similarity-bff/v1/related-artists/{artist_id}` | Replacement for dead `/artists/{id}/related-artists` |
| `analytics_top` | Phase 10 derivations | Tracks/artists/albums by window |
| `analytics_habits` | Phase 10 | Day/week/month rollups |
| `ops_log` | Phase 12 | Recent mutations |
| `undo_last` | Phase 12 `Request::OpsUndo` | Reverts last mutation (no confirm needed — undo is the safety net) |

## Resources to expose

- `spotuify://playback` — subscribable; refreshes on `DaemonEvent::PlaybackChanged`.
- `spotuify://devices` — refreshes on `DevicesChanged`.
- `spotuify://playlists` — refreshes on `PlaylistsChanged`.
- `spotuify://now_playing/lyrics` — live lyrics stream tied to current track and position.

## Confirmation pattern

Every destructive tool MUST take a `confirm: bool` argument:
- `false` (default) → returns a preview object (`MutationPreview` from Phase 12) and does NOT execute.
- `true` → executes, returns receipt.

This matches spotify-player commit #966 ("Add confirmation popups on destructive actions") for the TUI; we apply the same discipline at the MCP layer. An LLM that wants to confirm asks the user; the MCP server doesn't second-guess.

## Authentication & transport

- **stdio mode (default)**: trusts the process owner. Best for editor integrations.
- **HTTP mode**: `spotuify mcp --http 127.0.0.1:PORT` with `SPOTUIFY_MCP_TOKEN` bearer-token auth. For remote agents and harnesses.
- TLS not handled internally; expose via local-only address or a reverse proxy.
- Rate-limit MCP tool calls per (session, tool) to prevent agent loops from exhausting Spotify quota.

## Architecture

```text
crates/spotuify-mcp/
├── src/
│   ├── lib.rs
│   ├── server.rs           // JSON-RPC 2.0 over stdio/HTTP
│   ├── tools.rs            // tool catalogue + schemas
│   ├── resources.rs        // subscribable resources
│   ├── confirm.rs          // destructive-action gating
│   └── bridge.rs           // map MCP request → spotuify Request → MCP response
└── tests/
    └── mcp_handshake.rs    // golden manifest test
```

The MCP server is a thin bridge:
1. Receive MCP tool call.
2. Validate (`confirm` for destructive ops).
3. Translate to `spotuify-protocol::Request`.
4. Send to daemon over UDS.
5. Wait for `Response`.
6. Translate to MCP result.
7. Return.

Subscribed resources fan out `DaemonEvent`s as MCP `resource.updated` notifications.

## Agent playlist workflow clarification

`agent_playlists::build_playlist_plan` (in `spotuify-cli` post-split) is intentionally a deterministic scaffold heuristic, not an LLM call. The actual planning happens in the upstream agent (Claude, GPT, local model). MCP makes this explicit:

1. LLM proposes plan JSON matching `PlaylistPlan` schema.
2. LLM calls `playlist_resolve_tracks` MCP tool against the plan.
3. LLM calls `playlist_create` with `confirm: false` to preview.
4. LLM relays preview to user.
5. User approves.
6. LLM calls `playlist_create` with `confirm: true`.
7. Receipt comes back; LLM can call `undo_last` if user rejects after the fact.

Document this loop in README and in `09-agent-workflows.md`.

## Work items

1. Add `rmcp` (Rust MCP SDK) dependency, or hand-roll JSON-RPC 2.0 over stdio if SDK churns.
2. Define MCP tool catalogue and JSON Schemas in `spotuify-mcp/src/tools.rs`.
3. Bridge: MCP tool call → daemon Request → MCP tool result.
4. Bridge: `DaemonEvent` stream → MCP resource update notifications.
5. Add `spotuify mcp` subcommand. Default to stdio mode.
6. Add `--http <addr>` mode with bearer-token auth.
7. Add confirmation gating on every destructive tool with explicit error message guiding the LLM to ask the user.
8. MCP capability negotiation (tools, resources, prompts).
9. Mercury-bus tools: `lyrics`, `radio_start`, `related_artists` (Phase 9 dep).
10. Analytics tools: `analytics_top`, `analytics_habits` (Phase 10 dep).
11. Undo tool: `undo_last`, `ops_log` (Phase 12 dep).
12. Write README snippets:
    - Claude Code: `claude mcp add spotuify --command spotuify-mcp`
    - Cursor: `.cursor/mcp.json` example
    - Continue: `config.json` example
13. MCP manifest golden test.

## Verification

- `claude mcp add spotuify` succeeds; tools appear in `claude mcp list`.
- LLM can run `search` → `play` → `now_playing` end-to-end in a fresh Claude Code session.
- MCP manifest validates against current MCP spec.
- `playlist_create` with `confirm: false` returns a preview without mutating; `confirm: true` produces the same receipt as `spotuify playlist create --yes`.
- `playlist_add` without `confirm` returns a clear "confirmation required" error; LLM must explicitly re-call with `confirm: true`.
- `radio_start` returns a station of URIs sourced from mercury (`autoplay-enabled` + `radio-apollo`); does NOT call the dead Web API `/recommendations` endpoint.
- `lyrics` returns synced lines for a track that has them, plain text for tracks that don't, "not available" for missing ones.
- Killing the daemon while an MCP session is active surfaces a clear error and auto-recovers on next call.
- `undo_last` reverts the last destructive op visible via `ops_log`.

## Definition of done

A user installs `spotuify`, adds the MCP server to Claude Code, and asks "make me a focus playlist." Claude calls `playlist_plan` → `playlist_resolve_tracks` → `playlist_create --confirm:false` → user sees preview → Claude calls `playlist_create --confirm:true`. End-to-end works with no shell commands typed by the user. Mercury-bus tools (lyrics, radio, related-artists) work on `--backend embedded` and return clear errors on `--backend spotifyd`/`connect`. The MCP manifest validates and survives spec updates.
