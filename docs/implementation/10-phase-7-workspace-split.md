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
│   ├── spotuify-keychain/            # credential-storage leaf crate
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

## Dependency rules (compiler-enforced with documented pragmatic exceptions)

1. `spotuify-core` depends on nothing internal.
2. `spotuify-protocol` depends on `spotuify-core` only.
3. `spotuify-store` depends on `spotuify-core` and `spotuify-protocol`; `spotuify-search` depends on core/protocol/store.
4. `spotuify-spotify` depends on `spotuify-core`, `spotuify-protocol`, and `spotuify-keychain`.
5. `spotuify-player` depends on `spotuify-core`, `spotuify-spotify`, and `spotuify-audio` for embedded sink taps.
6. `spotuify-sync` depends on core/protocol/store/search/spotify/player.
7. `spotuify-system` depends on `spotuify-core` and `spotuify-protocol`.
8. `spotuify-lyrics` depends on core; daemon/store/player own cache and mercury access.
9. `spotuify-audio` depends on core only; `spotuify-player` may depend on `spotuify-audio` for the embedded sink tap.
10. `spotuify-daemon` is the integration point — depends on everything above.
11. `spotuify-cli` and `spotuify-tui` are moving toward protocol-only client boundaries, but current extraction keeps documented edges for daemon autostart and legacy shared helpers.
12. `spotuify-tui` may depend on `spotuify-audio` and other legacy client helper crates during extraction, but backend crates must not depend on TUI.
13. `spotuify-mcp` depends on `spotuify-protocol` and reaches the daemon over IPC like any other client.

## Work items (bottom-up)

1. [x] Convert root `Cargo.toml` to workspace with `members = ["crates/*"]`; declare shared `[workspace.dependencies]` for tokio/serde/anyhow versions.
2. [x] Move domain types (`MediaItem`, `Playlist`, `Device`, `Playback`, error enums) into `spotuify-core`.
3. [x] Move `src/protocol.rs` → `crates/spotuify-protocol/`.
4. [x] Move `src/store.rs` → `crates/spotuify-store/`.
5. [x] Move `src/search.rs`, `src/reindex.rs` → `crates/spotuify-search/`.
6. [x] Move `src/spotify.rs`, `src/auth.rs`, `src/config.rs` → `crates/spotuify-spotify/`; credential storage moved into `spotuify-keychain`.
7. [x] Move `src/spotifyd.rs` and create `spotuify-player::backends::{embedded, spotifyd, connect_only}` per Phase 9.
8. [x] Move `src/sync.rs` implementation → `crates/spotuify-sync/`.
9. [x] New crate `spotuify-system` (filled by Phase 14).
10. [x] New crate `spotuify-lyrics` (filled by Phase 16).
11. [x] New crate `spotuify-audio` (filled by Phase 17).
12. [x] Move daemon implementation → `crates/spotuify-daemon/`.
13. [x] Move CLI bits → `crates/spotuify-cli/`.
14. [x] Move TUI bits → `crates/spotuify-tui/`.
15. [x] New crate `spotuify-mcp` (filled by Phase 8).
16. [x] Reduce `src/main.rs` to dispatcher plus legacy adapter shims reusing client crates.
17. [x] Add workspace-boundary tests enforcing the dependency DAG and documenting extraction exceptions.
18. [x] Release packaging is tracked in Phase 11 cross-platform/release work, not required for the crate split itself.

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
- Workspace-boundary tests match the documented DAG and known extraction exceptions.
- `cargo build -p spotuify-mcp` succeeds without pulling `spotuify-tui`.
- `cargo build -p spotuify-cli` succeeds with documented daemon-autostart/helper edges.
- `cargo build --release --bin spotuify` produces a single binary that runs every existing CLI and TUI flow unchanged.
- `cargo check --features embedded-playback,rodio-backend` succeeds; `embedded-playback` without exactly one backend intentionally fails.
- Total LOC and build time recorded in CHANGELOG to track that the split doesn't bloat the project.
- `cargo test --test workspace_boundaries --quiet` passes; one root-boundary assertion remains ignored until the binary dispatcher loses its legacy adapter edges.

## Definition of done

Workspace matches the blueprint target plus the Phase 8/9/14/15/16/17
additions, with current extraction exceptions recorded in
`tests/workspace_boundaries.rs`. New feature areas now land in dedicated
crates, and `spotuify-core` / `spotuify-protocol` are usable without
pulling the TUI.
