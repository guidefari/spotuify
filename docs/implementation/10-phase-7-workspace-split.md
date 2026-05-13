# Phase 7 - Workspace Crate Split

## Goal

Move from single-crate to the target workspace layout in `blueprint/01-architecture.md` §"Target Rust workspace" so the daemon is embeddable, the MCP binary can share core, and module boundaries become compiler-enforced. Sized to accommodate the new crates introduced by Phases 8 (MCP), 9 (player backends), 14 (system integration), 15 (cover art), 16 (lyrics), and 17 (audio).

## Evidence base

Cross-checked against competitors:
- **spotify-player** workspace is two crates: `spotify_player` (bin) and `lyric_finder` (lib). The `lyric_finder` lib is actually unused by the binary — vestigial.
- **ncspot** workspace: root + `xtask` (dev tooling only). Single binary crate.
- **spotatui**: single binary, not a workspace.

**No competitor has a real workspace split.** This is a real differentiator for spotuify — it unlocks MCP-server embedding, library use by third parties, and clean Phase 9 backend swapping. The maintenance overhead is real but the alternative (4308-line god-struct, see spotatui `core/app.rs`) is worse.

## Target layout

```text
spotuify/
├── Cargo.toml                       # workspace root
├── crates/
│   ├── spotuify-core/               # domain types, IDs, errors, capabilities
│   ├── spotuify-protocol/           # Request/Response/Event for IPC + MCP
│   ├── spotuify-store/              # SQLite migrations + queries (Phase 6 freshness)
│   ├── spotuify-search/             # Tantivy indexing/query
│   ├── spotuify-spotify/            # Web API client + auth + compat normalizer (Phase 6)
│   ├── spotuify-player/             # PlayerBackend trait + embedded/spotifyd/connect impls (Phase 9)
│   ├── spotuify-sync/               # background sync + reconciliation (Phase 6)
│   ├── spotuify-system/             # MPRIS/notifications/hooks/Discord (Phase 14)
│   ├── spotuify-lyrics/             # mercury + LRCLIB providers (Phase 16)
│   ├── spotuify-audio/              # FFT + loopback for visualization (Phase 17)
│   ├── spotuify-daemon/             # socket server + handler + event broadcast
│   ├── spotuify-cli/                # clap commands + output renderers + selection
│   ├── spotuify-tui/                # ratatui frontend + cover widget (Phase 15)
│   └── spotuify-mcp/                # MCP server bridge (Phase 8)
└── src/main.rs                      # thin dispatch: tui | cli | daemon | mcp
```

## Dependency rules (compiler-enforced)

1. `spotuify-core` depends on nothing internal.
2. `spotuify-protocol` depends on `spotuify-core` only.
3. `spotuify-store`, `spotuify-search` depend on `spotuify-core` only.
4. `spotuify-spotify` depends on `spotuify-core`.
5. `spotuify-player` depends on `spotuify-core` and `spotuify-spotify`.
6. `spotuify-sync` depends on core/store/search/spotify/player.
7. `spotuify-system` depends on `spotuify-core` and `spotuify-protocol`.
8. `spotuify-lyrics` depends on core/store/player.
9. `spotuify-audio` depends on core/player.
10. `spotuify-daemon` is the integration point — depends on everything above.
11. `spotuify-cli` and `spotuify-tui` depend on `spotuify-protocol` only — never on store/search/provider internals.
12. `spotuify-tui` may depend on `spotuify-audio` (for FFT consumer wiring) but not vice versa.
13. `spotuify-mcp` depends on `spotuify-protocol` and reaches the daemon over IPC like any other client.

## Work items (bottom-up)

1. Convert root `Cargo.toml` to workspace with `members = ["crates/*"]`; declare shared `[workspace.dependencies]` for tokio/serde/anyhow versions.
2. Move domain types (`MediaItem`, `Playlist`, `Device`, `Playback`, error enums) into `spotuify-core`.
3. Move `src/protocol.rs` → `crates/spotuify-protocol/`.
4. Move `src/store.rs` → `crates/spotuify-store/`.
5. Move `src/search.rs`, `src/reindex.rs` → `crates/spotuify-search/`.
6. Move `src/spotify.rs`, `src/auth.rs`, `src/config.rs` → `crates/spotuify-spotify/`. **Add the Phase 6 compat normalizer module here**.
7. Move `src/spotifyd.rs` and create `spotuify-player::backends::{embedded, spotifyd, connect}` per Phase 9.
8. Move `src/sync.rs` → `crates/spotuify-sync/`.
9. New crate `spotuify-system` (empty initially; Phase 14 fills it).
10. New crate `spotuify-lyrics` (empty initially; Phase 16 fills it).
11. New crate `spotuify-audio` (empty initially; Phase 17 fills it).
12. Move `src/daemon/` → `crates/spotuify-daemon/`.
13. Move CLI bits → `crates/spotuify-cli/`.
14. Move TUI bits → `crates/spotuify-tui/`.
15. New crate `spotuify-mcp` (empty initially; Phase 8 fills it).
16. Reduce `src/main.rs` to dispatcher reusing the client crates.
17. Add `cargo-modules` or `cargo deny` CI check enforcing the dependency DAG.
18. `cargo dist` config in workspace root for the matrix release.

## Migration discipline

- Each PR moves ONE crate and keeps the binary building.
- No public-API breakage during the move (just re-exports).
- After move, prune `pub` items aggressively; only the dependency-DAG-aligned surface should be public.
- Document migration order in a CHANGELOG section.

## Shared `[workspace.dependencies]`

Pin once, depend everywhere with `workspace = true`:
- tokio, tokio-util
- serde, serde_json
- anyhow (until Phase 6's typed errors land, then minimal)
- tracing, tracing-subscriber
- chrono
- thiserror (for typed errors)

## Verification

- `cargo build --workspace` from clean succeeds.
- `cargo test --workspace` passes.
- `cargo modules generate graph --workspace` matches the §"Dependency rules" DAG; no back-edges.
- `cargo build -p spotuify-mcp` succeeds without pulling `spotuify-tui` or `spotuify-cli`.
- `cargo build -p spotuify-cli` succeeds without pulling `spotuify-store` or `spotuify-search`.
- `cargo build --release --bin spotuify` produces a single binary that runs every existing CLI and TUI flow unchanged.
- `cargo check --no-default-features --features embedded-playback` succeeds (Phase 9 prep).
- Total LOC and build time recorded in CHANGELOG to track that the split doesn't bloat the project.

## Definition of done

Workspace matches blueprint target plus the Phase 8/9/14/15/16/17 additions. CI enforces the dependency DAG. New phases can add code into their own crate without touching the binary's module graph. A future third-party can `cargo add spotuify-core spotuify-protocol` and build their own client.
