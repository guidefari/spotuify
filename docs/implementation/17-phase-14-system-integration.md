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
- `[events] hook_command = "..."` in config.
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

1. Add `crates/spotuify-system` to workspace.
2. souvlaki integration with hidden-window pattern for mac/win.
3. Cover-art file cache.
4. notify-rust integration with templated formatting and XDG hints.
5. Shell-hook dispatcher; document hook contract in `docs/agent-workflows.md`.
6. Discord RPC behind feature flag and config toggle.
7. Per-event configuration in `config.toml`.
8. Doctor reports MPRIS bus name, notification subsystem, hook command, Discord state.
9. CLI commands: `spotuify hooks test` (fires a sample event for hook debugging), `spotuify mpris status`.

## Verification

- Linux: `playerctl status` shows "Playing" when spotuify is playing; media key on keyboard pauses spotuify.
- macOS: media key (F8) pauses spotuify; Now Playing widget in Control Center shows current track + album art.
- Windows: media key pauses; SMTC overlay shows track.
- Headless macOS daemon (no TUI): daemon emits `MediaControlsUnavailable`, doesn't crash, doctor warns clearly.
- Notification fires on track change with cover art visible.
- `spotuify hooks test` invokes the configured hook command with a sample track.
- Discord RPC shows current track in Discord profile when enabled.
- Two spotuify daemons can run simultaneously; their MPRIS bus names differ by PID.

## Definition of done

Linux users get full MPRIS parity with the official Spotify client. macOS/Windows users get Now Playing/SMTC integration when running a TUI process. Headless deployments degrade gracefully and emit clear events. Notifications and Discord RPC available for users who want them, off by default for users who don't. Shell hooks let power users wire spotuify into Last.fm/tmux/Hammerspoon without modifying spotuify.
