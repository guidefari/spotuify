# macOS Rust Rewrite Plan

This is the planning doc for rewriting the current SwiftUI macOS client in Rust.

**Status of the Swift client:** frozen. It is no longer being actively maintained. It is a reference to mine for behavior, not a moving target to chase parity against. That lowers the risk of the rewrite: the inventory below does not shift under us.

**Decided (was open, now settled):**
- UI stack is **GPUI**.
- The desktop client is a **socket client of the daemon** via `spotuify-protocol`. It does not link `spotuify-daemon` in-process.

Source of truth for the current desktop surface inventory:
- The frozen Swift tree itself: `clients/macos/Sources/Spotuify/Views/**` and `clients/macos/Sources/SpotuifyKit/**`. There is no separate `mac-client-surfaces.md` in this repo; the code is the inventory. Section 1 below enumerates it so nothing is missed.

Reference Rust codebase for GPUI + crate-split patterns to copy:
- `/Users/guidefari/source/oss/notifications` (GPUI app; `notification-core` / `notification-app` / `notification-cli` split)
- Its own rewrite writeup: `/Users/guidefari/source/oss/notifications/RUST_GPUI_REWRITE.md` and `/Users/guidefari/source/oss/notifications/context.md` (the GPUI patterns, dev-loop ergonomics, and platform-module shape this plan leans on)

## 1. What exists today

The current macOS app is not just a shell. It already exposes a full product surface:

- Main player window with daemon gating, navigation shell, update banner, toasts, and due-reminder sheet.
- Playback chrome, including play/pause, next, previous, shuffle, repeat, volume, seek, device selection, queue, lyrics, and the now playing footer.
- Full now-playing stage with artwork, visualizer, lyrics, queue, minimize mode, and track metadata.
- Floating mini player with 3 sizes.
- Menu bar extra with compact transport and quick open/quit actions.
- Settings window with 9 panes.
- Theme preference, artwork accenting, and scene-level tinting.
- System integrations for media controls, keyboard shortcuts, notifications, and update checks.
- Cover art caching and artwork-derived color extraction.
- Dock/menu/window lifecycle behavior.

Beyond the headline chrome above, the frozen Swift tree also ships these surfaces, each of which is a parity obligation the rewrite must not silently drop:

- Search (`Views/Search`), Library (`Views/Library`), Playlists (`Views/Playlists`), Podcasts (`Views/Podcasts`), History (`Views/History`), Devices (`Views/Devices`), Queue (`Views/Queue`), Lyrics (`Views/Lyrics`), Detail views (`Views/Detail`).
- Reminders: full subsystem, not a footnote — `Views/Reminders` (list + picker), plus `System/ReminderNotificationScheduler` and a `RemindersStore`. Due-reminder sheet is user-visible.
- Visualizer (`Views/Common/VisualizerView` + `VizStore`).
- `SpotuifyKit`: the Swift-side IPC/state layer (`Networking/*` socket + frame codec, `Models/*` wire types, `Stores/*` per-domain stores). This is exactly the duplicated-domain-model layer the rewrite deletes by reusing `spotuify-core` + `spotuify-protocol` directly. That deletion is the strongest concrete win of the rewrite.

Use the frozen `Sources/` tree as the exhaustive inventory and keep this doc as the rewrite map. When in doubt about a behavior, read the Swift file — it is stable.

### Visual direction from `Spotify UI.html`

The bundled HTML design points at a simpler, more iconic brand language than the current SwiftUI chrome:

- Warm near-black background (`#1d1413`), not generic system gray.
- A single strong accent blue (`#4f7fe8`) for the core mark and highlights.
- Fraunces as the display face for the wordmark.
- Centered square logo geometry with a cutout circle, closer to an app icon / album tile than a dense UI mock.
- The overall feel is minimal, editorial, and branded, not busy or system-default.

That direction should influence the Rust rewrite's window shell, app icon treatment, and title treatment even if we keep the current Spotify-specific surface structure.

## 2. Patterns to copy from `notifications`

The `notifications` app is a good Rust rewrite model because it keeps the layers boring and explicit.

### Architectural patterns

- `core` crate owns domain types and data transforms.
- `app` crate owns UI state, refresh lifecycle, and view composition.
- `platform` module isolates macOS-specific effects.
- CLI stays separate and can validate the same domain logic.
- Diagnostics are first-class, not hidden in logs.
- File watching plus a timer fallback keeps refresh predictable.
- Read-only access is intentional and narrow.

### UI patterns worth copying

- One main window, not a pile of ad hoc windows.
- Sidebar selection drives a content pane.
- Permission failure is a dedicated view, not a toast.
- App state is centralized, then views render derived data.
- Theme loading happens once at startup, then synchronizes system appearance.
- Platform helpers handle app lookup, icons, and deep links.

### What maps cleanly to spotuify

- `notification-core` maps to a future `spotuify-desktop-core` or to existing shared crates if we keep UI logic thin.
- `notification-app` maps to a new Rust desktop app crate.
- `platform/macos.rs` maps to macOS-only helpers for app integration, file dialogs, media controls, and filesystem behavior.
- Diagnostics view maps to daemon, auth, player, and cache health reporting.

## 3. Why Rust

The rewrite motive is not novelty. It is fit.

### 1. Cross-platform by default

The current Swift client is macOS-only. The Rust workspace already targets daemon, CLI, and TUI surfaces across platforms. Moving the desktop client into Rust means the same core model can back macOS now and other desktops later without a second implementation language.

### 2. Faster iteration with LLMs

Rust's module boundaries, types, and crate seams make it easier to make a narrow change, compile, and verify the exact surface touched. That matters when the work is done with agents:

- smaller files are easier to inspect and rewrite safely
- explicit types make partial refactors less ambiguous
- `cargo check` and targeted tests give fast feedback loops
- the same Rust patterns already exist elsewhere in this repo, so the agent can reuse local conventions instead of re-learning SwiftUI state flow

### 3. Performance and predictability

This is not about chasing benchmark numbers. It is about predictable runtime behavior:

- lower overhead than a large SwiftUI view graph for a client that mostly renders daemon state
- less UI state duplication across scene roots
- easier bounded async work around IPC, refresh, and file watching
- one runtime and one toolchain across daemon, CLI, TUI, and desktop client logic

### 4. Architectural consistency

The rest of spotuify is already becoming a Rust workspace with explicit seams. Keeping the desktop client in Rust aligns the whole product around the same domain types, protocol, and diagnostics model.

### 5. Reuse over rewrites

Rust lets us pull more of the desktop app into shared crates instead of recreating the same logic in a second language. That is the real leverage: one domain model, one protocol, one set of tests, multiple clients.

## 4. Reuse map in spotuify

We already have a lot of Rust to reuse. The rewrite should lean on it hard.

### Direct reuse targets

- `spotuify-core`
  - Domain types for playback, media items, devices, playlists, library, search.
  - Keep this as the shared model layer.
- `spotuify-protocol`
  - IPC request, response, and event shapes.
  - This should be the primary contract between the desktop app and daemon.
- `spotuify-launcher`
  - Daemon lifecycle and socket probing.
  - Useful for app startup, reconnect, and compatibility checks.
- `spotuify-daemon`
  - Authoritative runtime state, events, and playback orchestration.
- `spotuify-store`
  - SQLite cache for metadata and local truth.
- `spotuify-search`
  - Tantivy search and rebuildable derived state.
- `spotuify-sync`
  - Background refresh orchestration.
- `spotuify-spotify`
  - Web API client, auth, image selection, and mutation wrappers.
- `spotuify-player`
  - Device and playback backend logic.
- `spotuify-system`
  - Media controls, notifications, cover art cache, and platform bridges.
- `spotuify-lyrics`
  - Lyrics fetch and parsing.
- `spotuify-cli`
  - Keep CLI parity as the validator for the rewrite.
- `spotuify-tui`
  - Good reference for state shaping, selection, and command semantics.

### Current spotuify UI code worth mining

- `clients/macos/Sources/Spotuify/Views/Shell/AppShell.swift`
- `clients/macos/Sources/Spotuify/Views/Player/NowPlayingView.swift`
- `clients/macos/Sources/Spotuify/Views/Player/NowPlayingBar.swift`
- `clients/macos/Sources/Spotuify/Views/Settings/SettingsView.swift`
- `clients/macos/Sources/Spotuify/Views/MiniPlayer/MiniPlayerView.swift`
- `clients/macos/Sources/Spotuify/Views/MenuBar/MenuBarView.swift`
- `clients/macos/Sources/Spotuify/Theme/*`

These files already encode the intended behavior. The rewrite should preserve the semantics, not the Swift syntax.

### Swift-era features to carry forward

The current Swift work already introduced the exact UX/state pieces we want to preserve in Rust:

- Theme preference persistence and app-wide color-scheme control.
- Artwork-derived accenting for the main chrome when adaptive is enabled.
- Dedicated themed surfaces for player, mini player, menu bar, and settings.
- Themed desktop shell with persistent now-playing footer and update/banner affordances.
- Settings appearance grid, not a plain radio list.
- Album-stage tokenization for text, scrims, and glass effects.
- Contrast-aware transport icons and metadata on light themes.
- A branded app icon / accent direction instead of the default generic look.
- Mini player size cycling and floating-window behavior.
- Menu bar quick actions and player launch shortcuts.
- Removal of the queue as a top-level sidebar destination in favor of a global rail.

These are the first things the rewrite should re-express in Rust, not optional polish.

## 5. Suggested target architecture

Closest `notifications`-style Rust shape:

```text
spotuify desktop app
  -> desktop app state
  -> spotuify-protocol client
  -> daemon
  -> store/search/player/system/spotify/lyrics
```

More detailed:

```text
UI crate (spotuify-desktop)
  -> app state and actions
  -> platform/macOS adapter
  -> spotuify-protocol client  ===[ Unix socket ]===>  daemon
                                                          -> store
                                                          -> search
                                                          -> player
                                                          -> sync
                                                          -> spotify
                                                          -> lyrics
                                                          -> system
```

The desktop crate depends on `spotuify-core` (types) and `spotuify-protocol` (wire) and `spotuify-launcher` (lifecycle/probing). It does **not** depend on `spotuify-daemon` or any of store/search/player/sync/spotify/lyrics/system. Those are reached only across the socket, exactly like `spotuify-tui` and `spotuify-cli`. This keeps the CLAUDE.md crate rule intact: clients must not link daemon internals.

### Recommended crate split for the rewrite

If we want a notifications-like split, add one desktop app crate and keep the rest shared:

```text
crates/
  spotuify-core
  spotuify-protocol
  spotuify-launcher
  spotuify-store
  spotuify-search
  spotuify-spotify
  spotuify-player
  spotuify-sync
  spotuify-system
  spotuify-lyrics
  spotuify-audio
  spotuify-daemon
  spotuify-cli
  spotuify-tui
  spotuify-mcp
  spotuify-desktop   <- new Rust desktop client
```

The desktop crate should stay thin and own only:

- window and view composition
- local UI state
- theme selection
- scene lifecycle
- platform adapters
- daemon status / diagnostics presentation

## 6. Feature parity slices

Build it in this order:

1. Main window shell
   - daemon gate
   - navigation shell
   - now playing footer
   - update banner and toasts
2. Playback and library affordances
   - play/pause, next, previous, shuffle, repeat, seek, volume
   - devices, queue, lyrics
   - like/save actions
3. Content surfaces (the bulk of the app — do not skip)
   - Search, Library, Playlists, Podcasts, History
   - Detail views for playlist/album/artist/track
   - Visualizer
4. Reminders subsystem
   - reminders list + picker
   - due-reminder sheet
   - reminder notification scheduling
5. Settings window
   - appearance
   - playback/audio
   - notifications/privacy
   - updates/daemon/about
6. Mini player
   - floating window
   - size modes
   - artwork-first presentation
7. Menu bar extra
   - compact control surface
   - open player / mini player / quit
8. System integration
   - media controls
   - notifications
   - keyboard shortcuts
   - cover art cache
9. Cleanup
   - remove SwiftUI app once parity is good

**First shippable milestone:** slices 1–2 plus one content surface (Search) is the minimum that a user could actually run instead of the Swift app for basic use. Ship/dogfood at that point rather than carrying two clients silently to the end. Since the Swift app is frozen, there is no pressure to reach full parity before switching — the Rust app can become the daily driver the moment it covers the user's core loop, and the remaining slices land incrementally.

**Kill criterion:** if GPUI churn or missing platform APIs make slices 1–2 cost more than a few focused sessions, stop and reassess the stack before sinking effort into slices 3+.

## 7. What to reuse from `notifications` specifically

The best patterns to copy are:

- One `AppView`-style root that owns derived state.
- A sidebar that renders a list of groups or sections.
- A permission/diagnostics fallback view when a critical resource is inaccessible.
- A refresh loop with a timer fallback and a separate activation-triggered refresh.
- A platform module for app names, icons, and open-url/open-app behavior.
- Small view files per surface, not one giant file.

## 8. Useful crates

### Already in spotuify and likely useful

- `spotuify-core`
- `spotuify-protocol`
- `spotuify-launcher`
- `spotuify-system`
- `spotuify-player`
- `spotuify-store`
- `spotuify-search`
- `spotuify-sync`
- `spotuify-spotify`
- `spotuify-lyrics`
- `spotuify-cli`

### If we choose a Rust desktop UI stack similar to `notifications`

- `gpui`
- `gpui-component`
- `winit` for window integration when needed

### If we stay closer to the current Rust app model

- `ratatui` for TUI reuse patterns
- `crossterm` for input ideas, not for desktop UI
- `tokio` for lifecycle and refresh
- `serde_json` for debug payloads and diagnostics

### Platform and media helpers

- `open`
- `dirs`
- `notify-rust`
- `souvlaki`
- `image`
- `reqwest`
- `url`
- `uuid`

### Development loop

Mirror the `notifications` ergonomics so the desktop client stays easy to build and verify while it is under construction.

Recommended commands:

```text
just build      -> cargo build --release -p spotuify-desktop
just run        -> open the built app or launch the desktop binary
just doctor     -> emit structured diagnostics for daemon/auth/ui state
just test       -> cargo test -p spotuify-desktop and the shared crates it touches
just bundle     -> package the macOS app/dmg once the UI is ready
```

The important part is not the exact command names. The important part is that we have one-command entry points for:

- build
- run
- diagnostics
- tests
- packaging

This keeps the rewrite fast to iterate on, the same way `notifications` does with `just build`, `just run`, and a separate doctor/dump path.

## 9. Suggested work plan

### Phase 0

Freeze the inventory. The frozen Swift tree under `clients/macos/Sources/` is the source of truth for current behavior — `Views/**` for surfaces, `SpotuifyKit/**` for state/IPC. Section 1 enumerates it; read the Swift file directly when a behavior is unclear. Since the Swift client is no longer maintained, this inventory is stable and does not need re-freezing.

### Phase 1

Pick the desktop UI stack. If we want the closest match to `notifications`, use GPUI and a new desktop crate.

### Phase 2

Extract any missing shared state into Rust core types. Avoid duplicating Swift-only semantics in the new UI layer.

### Phase 3

Build the desktop shell and diagnostics path first. That gives us a place to see daemon health, auth state, and scene routing early.

### Phase 4

Port the main player window and the settings window next. Those cover most of the app's logic and state coupling.

### Phase 5

Port mini player, menu bar, and system integrations.

### Phase 6

Remove the Swift client only after parity is good and CLI smoke checks still pass.

## 10. Open decisions

Settled (moved out of this list): UI stack is GPUI; the desktop client talks to the daemon over the socket via `spotuify-protocol`, not by linking the daemon crate.

Still open:

- Whether the desktop crate ships as a separate package or lives as a binary inside the workspace first.
- Whether settings and theme state live entirely in the daemon, or partly local to the client.
- GPUI's non-macOS story: the "cross-platform by default" motive in §3 is real for the shared crates, but GPUI itself is macOS/Linux-first with a weak Windows path. Decide whether Windows is actually a near-term target or just a someday-nice — the answer changes whether GPUI's limitations matter.

## 11. Working rule

Do not start by translating Swift line-for-line. Start from the feature inventory, then map each affordance to a Rust owner, a protocol call, or a local client state field.
