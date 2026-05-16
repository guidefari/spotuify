# Phase 14 - System Integration: MPRIS, Media Keys, Notifications

## Goal

Ship system-integration parity with ncspot, spotify-player, and spotatui so spotuify isn't a worse desktop citizen than the alternatives. Media keys, OS-level Now Playing, desktop notifications. Optional Discord Rich Presence.

## Evidence base

| Feature | Reference | Notes |
|---|---|---|
| MPRIS via souvlaki (Linux/macOS/Windows) | spotify-player `media_control.rs` | Souvlaki abstracts D-Bus / SMTC / NowPlaying |
| MPRIS via zbus directly (Linux only) | ncspot `mpris.rs` | More control but Linux-only; ncspot does this |
| macOS Now Playing | spotatui `infra/macos_media.rs` | Direct AVFoundation/MediaRemote |
| Notifications via notify-rust | ncspot `queue.rs:486-516`, spotify-player `client/mod.rs:1821-1903` | Both use cover-art-as-file-path approach |
| Discord Rich Presence | spotatui `infra/discord_rpc.rs` | discord-rich-presence crate; opt-in |
| Shell-hook event system | spotify-player `streaming.rs` (player_event_hook_command) | Unix-style extensibility |

## Decision: souvlaki vs direct platform APIs

**Adopt souvlaki for cross-platform media controls.** Use spotify-player's `media_control.rs` as template.

Pros:
- Single API across Linux MPRIS, macOS MediaRemote, Windows SMTC.
- Updates pushed once per second (capped, prevents D-Bus flooding).
- Cover-art update via file path.

Cons:
- Windows/macOS need a real window handle. Mitigation: hidden winit window when a UI process is alive; daemon-only mode (no UI) skips media controls on those platforms with a clear `DaemonEvent::MediaControlsUnavailable { reason }`.
- Daemon-only macOS/Windows mode = no media keys; document this limitation.

Reserve direct zbus MPRIS as a Linux-only fallback if souvlaki has bugs that block us, but start with souvlaki.

## Deliverables

### Media controls (souvlaki)
- `crates/spotuify-system/src/media_controls.rs`.
- Bus name: `spotuify.instance{pid}` (multi-instance safe).
- Handlers wire `MediaControlEvent::{Play, Pause, Toggle, Next, Previous, SetPosition, SetVolume, OpenUri}` → daemon `Request`.
- Updates pushed on every `DaemonEvent::PlaybackChanged`:
  - `playback_status` (Playing/Paused/Stopped)
  - `metadata` (track, artists, album, cover-art URL/path, duration_us, track_id)
  - `position_us`
  - `volume` (0.0-1.0)
  - `shuffle`, `repeat`
- Cap rate to 1 update/sec to avoid D-Bus flooding.
- `Seeked` signal emitted on user-initiated seek.

### Cover-art file caching for notifications & MPRIS
- Cover art URLs are downloaded once into `~/.cache/spotuify/covers/<basename>.jpg`.
- Pass file paths (`file://...`) to souvlaki and notify-rust.
- Cache eviction: LRU by file mtime, cap at 200MB default.

### Desktop notifications (notify-rust)
- `crates/spotuify-system/src/notifications.rs`.
- Toggleable via `[notifications] enabled = true` (default false to avoid surprising new users).
- Template: `[notifications] summary = "{track}"`, `body = "{artist} - {album}"`.
- Format tokens: `{track}`, `{artist}`, `{artists}` (comma-joined), `{album}`, `{duration}`, `{progress}`.
- Linux: XDG hints — `urgency=Low`, `transient=true`, `desktop-entry=spotuify` so notifications group nicely and don't accumulate.
- macOS: notify-rust supports NSUserNotification (may require updating to use UNUserNotificationCenter in future).
- Windows: notify-rust uses WinRT toast.
- Per-event toggles: `on_track_change`, `on_pause`, `on_resume`, `on_skip`, `on_error`.

### Shell-hook event system
- Adopt spotify-player's `player_event_hook_command` pattern.
- `[analytics] hook_command = "..."` in config; legacy `player.event_hook` remains a fallback.
- Invoked by daemon on every meaningful `DaemonEvent`:
  - `spotuify_hook track-change <uri> <name> <artist> <album> <duration_ms>`
  - `spotuify_hook playback-paused <uri> <position_ms>`
  - `spotuify_hook playback-resumed <uri> <position_ms>`
  - `spotuify_hook track-finished <uri> <reason: completed|skipped|errored>`
  - `spotuify_hook listen-qualified <uri> <duration_ms>`
- Process spawned with environment variables set: `SPOTUIFY_URI`, `SPOTUIFY_TRACK`, etc.
- Best-effort; failures logged but never block daemon.
- Use cases: external scrobbling (Last.fm, ListenBrainz, Maloja), tmux status updates, Hammerspoon/AutoHotkey hooks.

### Media keys
- Linux: routed through MPRIS via souvlaki — DE handles media keys.
- macOS: media keys captured by souvlaki's NowPlaying integration when window handle is alive.
- Windows: same via SMTC.
- Fallback: no global hotkey daemon. Document that users without a Connect-compliant DE/OS layer can use TUI key bindings.

### Discord Rich Presence (opt-in)
- Behind `[discord] enabled = false` (default off).
- Application ID configurable (`[discord] application_id = "..."`).
- Updates on track change; shows track, artist, elapsed time.
- spotatui ships this; spotify-player and ncspot don't.
- Crate: `discord-rich-presence`.

### Architecture

```text
crates/spotuify-system/
├── src/
│   ├── lib.rs
│   ├── media_controls.rs   // souvlaki bridge
│   ├── notifications.rs    // notify-rust bridge
│   ├── cover_cache.rs      // shared with Phase 15
│   ├── hooks.rs            // shell-hook dispatcher
│   └── discord.rs          // optional Discord RPC
```

The daemon owns a `SystemIntegration` actor that subscribes to `DaemonEvent` and fans out to media-controls / notifications / hooks / discord.

## Hidden-window pattern (macOS/Windows)

When the daemon is invoked WITHOUT a TUI process and souvlaki is needed (mac/win):
- Spawn a hidden winit window in a dedicated thread.
- Document that headless macOS/Windows daemon = no media key support.
- Provide `--no-media-controls` daemon flag to skip the hidden window entirely.
- For Linux, no window needed; D-Bus MPRIS works without it.

This is exactly spotify-player's approach (`media_control.rs:160-263`).

## Work items

1. [x] Add `crates/spotuify-system` to workspace. Verified by workspace-boundary tests and `spotuify-system` crate checks.
2. [x] Media-controls boundary now opens souvlaki, attaches OS media-key events, forwards mapped commands through the daemon playback request path, and keeps PID-scoped MPRIS bus names. Basic playback-state updates are pushed from `PlaybackChanged` events. Windows hidden-window lifecycle, rich track metadata/art updates, and live OS smoke checks remain follow-ups.
3. [x] Cover-art file cache implemented with TTL, size cap, integrity checks, and daemon cache-status reporting. Verified by `crates/spotuify-system/src/cover_cache.rs` tests.
4. [x] notify-rust notification bridge exists behind the `notifications` feature with templates, per-event toggles, Linux hints, daemon config wiring, and `spotuify config get/set notifications.*` support. Current daemon events still provide action labels rather than full track metadata or cover-art paths, so rich notification payloads remain a follow-up. Verified by notification template tests, `system_integration_sections_from_partial_toml_keep_defaults`, `config_set_and_get_supports_notification_keys`, and `system_config_includes_notification_preferences`.
5. [x] Shell-hook dispatcher exists and daemon system config now wires `[analytics] hook_command` / `hook_timeout_ms` into `SystemIntegration`; documented legacy `player.event_hook` works as a fallback. `listen-qualified`, track-change/start, and track-finished projections are wired from current `DaemonEvent`s; pause/resume still need richer playback events with URI/position. Verified by hook projection tests, `system_config_includes_analytics_hook_command`, `system_config_uses_player_event_hook_as_legacy_fallback`, and `system_config_prefers_analytics_hook_over_legacy_player_event_hook`.
6. [x] Discord RPC remains feature-gated config/handle scaffolding only. Live Discord IPC presence updates are deliberately not shipped until playback events carry enough track metadata; shipping URI-only presence would be a low-value half-feature.
7. [x] Per-event notification configuration is exposed in `config.toml` and the config CLI. Discord config remains parsed/scaffolded only because live Discord presence is not shipped yet.
8. [x] Doctor/cache status reports cover-cache stats, media-control enabled state and bus name, hook command/timeout, notification state, and Discord state. Verified by system diagnostics coverage plus daemon/system clippy.
9. [x] `spotuify hooks test` is shipped and runs the configured hook in strict mode with a sample listen-qualified event. Verified by CLI parse/help snapshots and `fire_checked_reports_spawn_failure`.
10. [x] `spotuify mpris status` prints the daemon media-control/system diagnostics already used by doctor. Verified by `mpris_status_command_accepts_machine_output_format`, help snapshots, bin clippy, and `spotuify-cli` clippy.

## Verification

- Linux `playerctl` and macOS Now Playing live smoke remain manual; Windows SMTC awaits the hidden-window driver.
- Headless Windows graceful degradation remains a follow-up; the current implementation does not claim Windows media-key support without a window handle.
- Notification rendering, config parsing, config CLI get/set, and daemon config wiring are covered; track metadata and cover-art notification smoke remain pending.
- Hook projection, daemon config wiring, and `hooks test` CLI parsing/strict execution are covered by tests.
- Discord RPC is scaffolded only; live profile smoke remains pending.
- PID-scoped bus-name construction and souvlaki attach exist, but two-daemon MPRIS smoke remains pending.

## Definition of done

The shipped Phase 14 slice provides the workspace crate, cover cache, working shell-hook dispatch/test CLI, MPRIS status CLI, and live Linux/macOS media-control command forwarding for currently available daemon events. Full Windows SMTC parity, rich notification/media metadata, and Discord live presence remain explicit follow-ups rather than shipped behavior.
