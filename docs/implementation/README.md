# spotuify - Implementation Plan

This directory is the execution plan for the blueprint.

The plan is phased so each phase leaves the app more useful than before. Do not skip CLI verification. Do not add TUI-only behavior.

## Documents

| # | Document | Purpose |
|---|---|---|
| 00 | [Current State](00-current-state.md) | What exists today and what is broken or missing |
| 01 | [Phase 0 - Stabilize](01-phase-0-stabilize.md) | Fix current app and establish smoke checks |
| 02 | [Phase 1 - CLI Parity](02-phase-1-cli-parity.md) | Extract shared actions and add CLI for current features |
| 03 | [Phase 2 - Daemon Protocol](03-phase-2-daemon-protocol.md) | Add daemon, IPC, CLI/TUI clients |
| 04 | [Phase 3 - Local Store and Search](04-phase-3-local-store-search.md) | SQLite cache, sync, Tantivy index |
| 05 | [Phase 4 - TUI Redesign](05-phase-4-tui-redesign.md) | Player-first mxr-style TUI UX |
| 06 | [Phase 5 - Agent Playlists](06-phase-5-agent-playlists.md) | Research/preview/commit playlist workflows |
| 07 | [Testing and Conformance](07-testing-conformance.md) | CLI/TUI/protocol test strategy |
| 08 | [mxr Reuse Map](08-mxr-reuse-map.md) | Concrete source areas to copy/adapt from mxr |
| 09 | [Phase 6 - Sync Hardening](09-phase-6-sync-hardening.md) | Rate limits, snapshot_id/ETag, freshness, two-stage receipts |
| 10 | [Phase 7 - Workspace Split](10-phase-7-workspace-split.md) | Extract single crate into target workspace layout |
| 11 | [Phase 8 - MCP Server](11-phase-8-mcp-server.md) | Expose daemon as Model Context Protocol server |
| 12 | [Phase 9 - Librespot Embed](12-phase-9-librespot-embed.md) | Decision gate: embed librespot vs supervise spotifyd |
| 13 | [Phase 10 - Analytics Derivations](13-phase-10-analytics-derivations.md) | Derived listening facts, top-N, habits, exports |
| 14 | [Phase 11 - Cross-Platform](14-phase-11-cross-platform.md) | Linux/Windows support, installers, releases |
| 15 | [Phase 12 - Operation Log and Undo](15-phase-12-operation-log-undo.md) | Recorded mutations with reversal plans |
| 16 | [Phase 13 - Spec Compliance and QoL](16-phase-13-spec-compliance.md) | Reload, reconnect, `-o` override, panic handling, decision-log backfill |
| 17 | [Phase 14 - System Integration](17-phase-14-system-integration.md) | MPRIS, media keys, notifications, Discord RPC, shell hooks |
| 18 | [Phase 15 - Cover Art](18-phase-15-cover-art.md) | Inline cover art in TUI (kitty/iTerm/sixel/halfblocks) |
| 19 | [Phase 16 - Lyrics](19-phase-16-lyrics.md) | Spotify-mercury + LRCLIB synced lyrics with offset tuning |
| 20 | [Phase 17 - Audio Visualization](20-phase-17-audio-visualization.md) | Sink-tap or loopback FFT spectrum in Player tab |

## Implementation rules

1. CLI first or same time as TUI.
2. Shared action/protocol layer before UI-specific code.
3. Every external dependency has a timeout.
4. Every broad mutation has dry-run or explicit reason why impossible.
5. Every machine-readable output has a stable schema.
6. Every phase includes commands an agent can run to verify it.
7. Before implementing daemon, IPC, SQLite, Tantivy, output formats, or TUI async flow, inspect mxr and copy/adapt the proven code path.
