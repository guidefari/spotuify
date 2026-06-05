# Player Sync and Observability Improvement Plan

Date: 2026-05-16
Status: Superseded by shipped player hot-path work
Owner: spotuify player reliability work

## Current status as of 2026-06-02

This plan is kept as the investigation ledger, not the current implementation
plan. The main direction shipped: the daemon owns `PlaybackClock`, clients seed
from daemon truth, playback mutations emit daemon-owned optimistic events, and
the embedded player has a dedicated transport lane for play/pause/next/previous
seek/volume before falling back to Spotify Web API reconciliation.

Still valid: player actions are the product hot path; Web API polling is a
fallback/reconciliation source; TUI state should render daemon truth rather
than local guesses.

Out of date: transport commands no longer wait only behind the ordinary Spotify
Web API mutation body. `Request::PlaybackCommand` now freezes the effective
command before optimistic state changes, tries local embedded transport within
a bounded fast window, and the player actor checks transport before ordinary
player commands and warm preloads.

## Summary

This document records the static analysis and external research behind a focused
player reliability repair pass for `spotuify`.

The user-visible problem is not one bug. It is a cluster of state ownership
problems:

- The bottom player can show a different title or cover art than the audio that
  is actually playing.
- The queue often appears to know the current track more accurately than the
  player panel.
- The seek bar can drift from the real audio position.
- Volume can be stale relative to the actual active device volume.
- Lyrics can be synced to a stale or wrong progress value.
- The visualizer is usually invisible because the active audio source is often
  `none`.
- Play/pause can feel slow because control and confirmation still flow through
  Spotify Web API paths and eventual consistency windows.

The highest-confidence finding from static analysis is that the app currently
combines multiple eventually-consistent snapshots as if they were one coherent
player state. Playback, queue, devices, lyrics, cover art, and visualization
state each have separate read/refresh/apply paths. That makes stale state
expected, especially after mutations.

The target fix is a daemon-owned playback truth model:

1. Add traces first so the issue can be measured from CLI, daemon, Spotify calls,
   cache refreshes, and TUI apply points.
2. Persist/use post-command playback results before notifying subscribers.
3. Introduce a daemon `PlaybackClock` that derives current position from a base
   sample and a monotonic clock.
4. Feed that clock from local player events when `spotuify` owns playback,
   command results when available, and Web API polling only as fallback or
   reconciliation.
5. Make TUI render one canonical now-playing item rather than mixing playback,
   queue, recent, lyrics, and cover snapshots opportunistically.

This is repair work, not a new feature. It directly supports the product
principle that player reliability is not optional.

## Current Architecture Relevant To This Bug

The target architecture says the daemon is the system. TUI, CLI, scripts, and
agents should be clients. The current repo is already partly split into crates,
but the player state path still contains legacy cache and polling assumptions.

Relevant crates and files:

- CLI args and command dispatch: `src/main.rs`,
  `crates/spotuify-cli/src/commands.rs`
- Protocol: `crates/spotuify-protocol/src/lib.rs`
- Daemon request handling: `crates/spotuify-daemon/src/handler.rs`
- Daemon state/player actor: `crates/spotuify-daemon/src/state.rs`
- Spotify Web API client/actions: `crates/spotuify-spotify/src/client.rs`,
  `crates/spotuify-spotify/src/actions.rs`
- Store snapshots: `crates/spotuify-store/src/lib.rs`
- TUI state/rendering: `crates/spotuify-tui/src/app.rs`,
  `crates/spotuify-tui/src/ui.rs`
- Player backends: `crates/spotuify-player/src/lib.rs`,
  `crates/spotuify-player/src/backends/*`
- Visualizer coordinator: `crates/spotuify-daemon/src/viz_coordinator.rs`

Existing docs already point at the intended direction:

- `docs/blueprint/07-player.md` says the player must be reliable and the daemon
  should ensure the preferred device is running and visible.
- `docs/implementation/09-phase-6-sync-hardening.md` says playback truth should
  come from a `PlayerEvent` stream, not polling.
- `docs/implementation/12-phase-9-librespot-embed.md` says embedded librespot
  should become the low-latency player path.
- `docs/implementation/20-phase-17-audio-visualization.md` says visualizer data
  should come from a sink tap when embedded, or explicit loopback otherwise.

## Static Analysis Findings

### CLI command flow

`status` flow:

```text
src/main.rs
  -> crates/spotuify-cli/src/commands.rs::ipc_status
  -> Request::PlaybackGet
  -> crates/spotuify-daemon/src/handler.rs
  -> store.latest_playback_or_recent()
  -> spawn_playback_refresh()
```

Key observation: `PlaybackGet` intentionally does not call Spotify inline. It
returns the most recent cached playback snapshot, falls back to recently played,
then spawns a background refresh.

Source refs:

- `src/main.rs:762`
- `crates/spotuify-cli/src/commands.rs:15`
- `crates/spotuify-protocol/src/lib.rs:71`
- `crates/spotuify-daemon/src/handler.rs:73`
- `crates/spotuify-store/src/lib.rs:344`

`queue` flow:

```text
src/main.rs
  -> crates/spotuify-cli/src/commands.rs::ipc_queue
  -> Request::QueueGet
  -> crates/spotuify-daemon/src/handler.rs
  -> store.latest_queue()
  -> spawn_queue_refresh()
```

Key observation: `QueueGet` has the same cache-first design. Queue and playback
snapshots can be from different Spotify polls and different cache insertion
times.

Source refs:

- `crates/spotuify-cli/src/commands.rs:64`
- `crates/spotuify-protocol/src/lib.rs:103`
- `crates/spotuify-daemon/src/handler.rs:287`
- `crates/spotuify-store/src/lib.rs:413`

Transport mutation flow:

```text
src/main.rs
  -> crates/spotuify-cli/src/commands.rs::ipc_playback_command
  -> Request::PlaybackCommand
  -> daemon optimistic mutation wrapper
  -> playback_command_kind()
  -> execute_with_device_recovery()
  -> spotuify_spotify::actions::execute()
  -> Spotify Web API method
```

Affected commands:

- `play-uri`
- `pause`
- `resume`
- `toggle`
- `next`
- `previous`
- `seek`
- `volume`
- `shuffle`
- `repeat`

Source refs:

- `src/main.rs:794`
- `crates/spotuify-cli/src/commands.rs:233`
- `crates/spotuify-protocol/src/lib.rs:315`
- `crates/spotuify-daemon/src/handler.rs:98`
- `crates/spotuify-daemon/src/handler.rs:1744`
- `crates/spotuify-daemon/src/handler.rs:2134`
- `crates/spotuify-spotify/src/actions.rs:196`
- `crates/spotuify-spotify/src/client.rs:615`

### Daemon discards post-command playback refresh

`spotuify_spotify::actions::execute()` refreshes playback after many transport
commands:

- pause
- resume
- toggle
- play item
- play URI
- next
- previous
- seek
- volume
- shuffle
- repeat

Example source refs:

- `crates/spotuify-spotify/src/actions.rs:202`
- `crates/spotuify-spotify/src/actions.rs:214`
- `crates/spotuify-spotify/src/actions.rs:232`
- `crates/spotuify-spotify/src/actions.rs:244`
- `crates/spotuify-spotify/src/actions.rs:257`
- `crates/spotuify-spotify/src/actions.rs:372`

But the daemon command body ignores the returned `CommandResult.playback`. It
only emits `DaemonEvent::PlaybackChanged`, then subscribers re-read cached
playback.

Source refs:

- `crates/spotuify-daemon/src/handler.rs:127`
- `crates/spotuify-daemon/src/handler.rs:133`

Why this matters:

1. User presses pause.
2. Daemon sends Web API pause.
3. `actions::execute()` may get a refreshed playback response.
4. Daemon discards it.
5. Daemon emits `PlaybackChanged`.
6. TUI re-fetches `PlaybackGet`.
7. `PlaybackGet` returns old cached state unless the background refresh has
   already persisted a newer snapshot.

This creates a direct path to stale pause/play status, stale volume, and stale
track metadata after mutations.

### Status is intentionally stale

`PlaybackGet` is designed to be fast. It never blocks on Spotify. That is good
for UI responsiveness, but it currently has no explicit freshness contract in
the protocol response.

The store fallback is:

1. latest row from `playback_snapshots`
2. synthesized paused playback from `recent_items`
3. empty default playback

Source refs:

- `crates/spotuify-daemon/src/handler.rs:73`
- `crates/spotuify-store/src/lib.rs:344`
- `crates/spotuify-store/src/lib.rs:380`

Problem: the CLI and TUI consume this as "current playback", not "best cached
guess". There is no `sampled_at_ms`, `cache_age_ms`, or source field in
`spotuify_core::Playback`.

Source ref:

- `crates/spotuify-core/src/lib.rs:26`

### Relative seek uses stale progress

Relative seek is especially risky:

```text
spotuify seek +15s
  -> read current playback through PlaybackGet
  -> parse +15s against cached progress_ms
  -> send absolute Seek { position_ms }
```

Source refs:

- `src/main.rs:810`
- `crates/spotuify-cli/src/commands.rs:240`
- `crates/spotuify-spotify/src/selection.rs:192`

Because `PlaybackGet` is cache-first, relative seek can use a progress value
that is seconds or minutes old. The resulting absolute seek can land far away
from user intent.

### Transport mutation lock can add perceived lag

The daemon uses mutation locks for transport requests. The lock protects
ordering, but it can also delay later commands behind a slow command.

Source refs:

- `crates/spotuify-daemon/src/state.rs:346`
- `crates/spotuify-daemon/src/handler.rs:36`

The optimistic mutation wrapper returns queued receipts, but the lock is
associated with the command body and can still serialize slow work. If a Spotify
call, token refresh, device recovery, or rate-limit sleep happens while holding
the transport lane, the next pause/next/seek can feel slow before it is even
accepted.

### Rate limit sleeps can affect transport

Playback control uses higher priority than background sync, but it still flows
through the rate-limited client and honors backoff.

Source refs:

- `crates/spotuify-spotify/src/client.rs:1201`
- `crates/spotuify-spotify/src/rate_limit.rs:291`
- `crates/spotuify-spotify/src/rate_limit.rs:342`

If a backoff is active for the relevant endpoint/scope, a command may sleep
before it reaches Spotify. This must be visible in logs.

### TUI uses split state

The TUI stores these independently:

- `playback`
- `queue`
- `devices`
- `last_played`
- `current_art_url`
- `cover`
- `lyrics`
- `lyrics_track_uri`
- `lyrics_offset_ms`
- visualization fields

Source ref:

- `crates/spotuify-tui/src/app.rs:152`

The event loop:

- redraws continuously
- polls every 4 seconds
- locally advances progress every 250ms
- listens for daemon events

Source refs:

- `crates/spotuify-tui/src/app.rs:1123`
- `crates/spotuify-tui/src/app.rs:1146`
- `crates/spotuify-tui/src/app.rs:1147`
- `crates/spotuify-tui/src/app.rs:1200`

The refresh fetches playback, queue, devices, and other reads concurrently, then
joins cover and lyrics before applying the full snapshot.

Source refs:

- `crates/spotuify-tui/src/app.rs:1314`
- `crates/spotuify-tui/src/app.rs:1335`
- `crates/spotuify-tui/src/app.rs:1421`

Problem: a single `RefreshSnapshot` can contain playback, queue, lyrics, cover,
and devices whose underlying data was sampled at different times.

### Current track and queue current can disagree

Bottom player title source:

```rust
app.playback.item.as_ref().or(app.last_played.as_ref())
```

Source ref:

- `crates/spotuify-tui/src/ui.rs:590`

Queue current source:

```rust
app.queue.currently_playing
```

Source ref:

- `crates/spotuify-tui/src/ui.rs:955`

Queue fullscreen mixes queue title with playback progress:

- title/item from `queue.currently_playing` or `playback.item`
- progress from `playback.progress_ms`

Source refs:

- `crates/spotuify-tui/src/ui.rs:426`
- `crates/spotuify-tui/src/ui.rs:486`

If the queue snapshot is fresher than playback, the queue shows the real current
track while the bottom player still shows the old cached playback track. If the
queue snapshot is stale, queue fullscreen can display progress from a different
track.

### Cover art can stay stale

Cover fetch only updates `app.cover` on success. If the track changes and the
new cover fetch fails, the previous cover can remain while playback exists.

Source refs:

- `crates/spotuify-tui/src/app.rs:692`
- `crates/spotuify-tui/src/app.rs:1481`
- `crates/spotuify-tui/src/ui.rs:561`

Correct behavior should clear or mark the cover as pending as soon as the active
art URL changes. Failed cover fetch should fall back to deterministic generated
art for the current URI, not keep the old image.

### Progress merge ignores same-track seek or drift

TUI local progress advances every 250ms while playing.

Source ref:

- `crates/spotuify-tui/src/app.rs:750`

`merge_playback()` only re-anchors `progress_ms` when track/play/shuffle/repeat
or device changes. It preserves local progress otherwise.

Source refs:

- `crates/spotuify-tui/src/app.rs:774`
- `crates/spotuify-tui/src/app.rs:809`

That avoids Web API latency yanking the progress bar backwards every poll, but
it also means remote seeks or large clock drift on the same track can persist
indefinitely.

### Lyrics can be applied to the wrong visible track

Lyrics fetch is keyed off the current playback URI at refresh planning time.

Source refs:

- `crates/spotuify-tui/src/app.rs:1282`
- `crates/spotuify-tui/src/app.rs:1450`

`apply_refresh()` applies returned lyrics without verifying that the lyrics
track URI still matches the currently displayed playback item.

Source ref:

- `crates/spotuify-tui/src/app.rs:714`

Render uses `app.playback.progress_ms` to choose active lyric line.

Source refs:

- `crates/spotuify-tui/src/ui.rs:1186`
- `crates/spotuify-tui/src/ui.rs:1288`

Therefore lyrics can be wrong because:

- the lyrics object is for a stale URI
- the progress clock is stale
- a remote seek occurred and `merge_playback()` preserved old local progress

### Volume uses playback device snapshot

Bottom player volume uses:

```rust
app.playback.device.as_ref().and_then(|device| device.volume_percent)
```

Source ref:

- `crates/spotuify-tui/src/ui.rs:703`

The devices tab uses the separate `DevicesList` cache.

Source refs:

- `crates/spotuify-daemon/src/handler.rs:142`
- `crates/spotuify-tui/src/ui.rs:1875`

Volume commands do not apply a local optimistic update to `app.playback.device`.

Source ref:

- `crates/spotuify-tui/src/app.rs:2964`

If playback and device snapshots disagree, the bottom bar and real device volume
can be out of sync.

### Visualizer is usually invisible for expected reasons

Config defaults say visualizer is enabled and source is auto.

Source refs:

- `crates/spotuify-spotify/src/config.rs:313`
- `crates/spotuify-spotify/src/config.rs:321`

But `VizCoordinator::activate_source()` deliberately sets active source to none
when:

- source is `auto`
- no embedded sink tap is available
- explicit loopback was not selected

Source refs:

- `crates/spotuify-daemon/src/viz_coordinator.rs:193`
- `crates/spotuify-daemon/src/viz_coordinator.rs:223`

This is a good safety decision on macOS because auto-opening default input can
interfere with audio. But the UI needs to make it obvious:

- visualizer is enabled
- active source is none
- why it is none
- how to get moving bars: embedded backend, or explicit loopback setup

### Logging and tracing gaps

Current logging:

- daemon logs to file
- JSON format available through `--log-format json` or
  `SPOTUIFY_LOG_FORMAT=json`
- filter uses `SPOTUIFY_LOG`
- background task lifecycle has some trace logging

Source refs:

- `crates/spotuify-daemon/src/logging.rs:8`
- `crates/spotuify-daemon/src/logging.rs:31`
- `crates/spotuify-daemon/src/logging.rs:41`
- `crates/spotuify-daemon/src/state.rs:560`

Gaps:

- service files use `RUST_LOG=info`, but active logging expects `SPOTUIFY_LOG`
- protocol has `SPOTUIFY_LOG_DIR`, active log path does not honor it
- no single IPC request span with request kind/source/duration/outcome
- weak logging for cache age and background refresh results
- `spawn_playback_refresh()` silently returns if `spotify_client()` fails

Source refs:

- `install/systemd/user/spotuify-daemon.service:11`
- `install/launchd/dev.spotuify.daemon.plist:24`
- `crates/spotuify-protocol/src/paths.rs:133`
- `crates/spotuify-daemon/src/handler.rs:1541`

## External Research

### Spotify Web API playback state

Official docs:

- Current playback:
  https://developer.spotify.com/documentation/web-api/reference/get-information-about-the-users-current-playback
- Queue:
  https://developer.spotify.com/documentation/web-api/reference/get-queue
- Playback mutations:
  https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback
- Devices:
  https://developer.spotify.com/documentation/web-api/reference/get-a-users-available-devices
- Rate limits:
  https://developer.spotify.com/documentation/web-api/concepts/rate-limits

Relevant findings:

- `GET /me/player` returns current item, device, `is_playing`,
  `progress_ms`, shuffle/repeat state, and a timestamp-like sample of playback
  state.
- `GET /me/player/queue` returns `currently_playing` plus upcoming queue.
- Playback mutation endpoints generally return success for command acceptance,
  not proof that every client has observed and applied the new state.
- Spotify warns that ordering across Player endpoints is not guaranteed when
  endpoints are used together.
- Device IDs are not a permanent identity contract. They are persistent only to
  some extent.
- Restricted devices cannot accept Web API commands.
- Rate limits are calculated over a rolling 30 second window, and 429 responses
  normally include `Retry-After`.

Implication for `spotuify`:

The Web API is appropriate for cross-device remote control and reconciliation.
It is not appropriate as the sole source of sub-second player truth. Polling it
more often can worsen rate limits without solving eventual consistency.

### Spotify Web Playback SDK

Official docs:

- https://developer.spotify.com/documentation/web-playback-sdk

Relevant findings:

- It is browser JavaScript.
- It exposes evented player state.
- It requires Premium and browser media constraints.

Implication for `spotuify`:

This is not a good core path for a Rust daemon/TUI. It is relevant as proof that
evented local playback state is the right product shape, but not the right
implementation substrate.

### librespot and spotifyd

References:

- librespot playback player source:
  https://docs.rs/librespot-playback/latest/src/librespot_playback/player.rs.html
- spotifyd D-Bus/MPRIS docs:
  https://docs.spotifyd.rs/advanced/dbus.html

Relevant findings:

- librespot exposes event-style playback information such as playing, paused,
  track changed, seeked, position correction, end of track, and session events.
- spotifyd is a librespot-based Spotify Connect receiver.
- spotifyd can expose MPRIS when built and enabled.

Implication for `spotuify`:

When playback is on the preferred local `spotuify-hume` device, the best truth
source is the local player event stream, not Web API polling. That aligns with
the existing Phase 9 embedded librespot plan.

### MPRIS

Official spec:

- https://specifications.freedesktop.org/mpris/latest/Player_Interface.html

Relevant findings:

- MPRIS exposes playback status, metadata, volume, and position control.
- `Seeked` events are useful for re-anchoring position.
- Normal progress changes should not be expected as continuous property-change
  events. Clients usually derive progress locally from the last known position.

Implication for `spotuify`:

MPRIS can be a useful fallback or system integration path, especially on Linux,
but it does not eliminate the need for a daemon playback clock.

### Lyrics

Relevant facts:

- Spotify Web API has no official lyrics endpoint.
- Existing project docs already treat lyrics as provider-backed, optional, and
  outside core Spotify Web API guarantees.

Source ref:

- `docs/blueprint/07-player.md:53`

Implication for `spotuify`:

Lyrics should be synced against the daemon playback clock. They should never be
driven by raw Web API polling progress alone.

### Audio visualization

Relevant references:

- Spotify Web API changes:
  https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api
- Spotify Developer Policy:
  https://developer.spotify.com/policy
- Existing spotuify plan:
  `docs/implementation/20-phase-17-audio-visualization.md`

Relevant findings:

- New/development access to Spotify Audio Analysis and Audio Features has been
  restricted.
- Real-time visualization should come from actual audio samples, not Spotify's
  deprecated or restricted analysis APIs.
- The technically best source is a local sink tap when `spotuify` owns audio
  through embedded librespot.
- Loopback can work but is platform-specific. macOS needs explicit user setup
  such as BlackHole or another virtual device.
- Spotify policy review is needed before shipping anything that could be
  considered synchronizing Spotify sound recordings with visual media.

Implication for `spotuify`:

The visualizer should be treated as local audio diagnostics until policy and UX
are clear. The immediate repair is to explain why no source is active and make
the status observable.

## Root Cause Hypotheses

### H1: Multiple state snapshots are treated as one current state

Confidence: high.

Evidence:

- playback, queue, devices, lyrics, and cover are fetched separately
- daemon reads are cache-first
- TUI renders mixed fields without checking sample times or URI agreement

Expected symptoms:

- queue shows real current track while bottom player shows old title
- cover art from old track remains
- volume in bottom player differs from devices/actual device
- queue fullscreen shows one track title with another track's progress

### H2: Post-command refreshed playback is discarded

Confidence: high.

Evidence:

- Spotify actions refresh playback after commands
- daemon ignores `CommandResult.playback`
- subscribers re-read stale cached playback

Expected symptoms:

- pause/resume UI lags
- seek bar does not jump after seek
- volume does not update immediately
- next/previous can show old track until later refresh

### H3: Relative seek is computed from stale cache

Confidence: high.

Evidence:

- CLI relative seek reads `PlaybackGet`
- `PlaybackGet` is cache-first
- `Playback` has no sample timestamp

Expected symptom:

- `spotuify seek +15s` lands somewhere surprising.

### H4: TUI progress smoothing hides real external seeks

Confidence: medium-high.

Evidence:

- TUI preserves local progress on same track unless track/play/shuffle/repeat or
  device changes
- remote seek on same track is not a resync reason

Expected symptoms:

- seek bar and lyrics drift after remote device seek
- correcting poll arrives but is ignored because track did not change

### H5: Visualizer is configured on but has no source

Confidence: high.

Evidence:

- default visualizer enabled
- default source auto
- auto resolves to none without embedded sink
- loopback is explicit opt-in

Expected symptom:

- visualizer area may render flat/no bars while status says enabled.

### H6: Web API latency is real, but app layering amplifies it

Confidence: high.

Evidence:

- Spotify Web API is polling/eventually consistent
- command endpoints can return before all clients agree
- app discards refreshed command state
- app then asks cached reads to catch up

Expected symptom:

- play/pause feels slower in `spotuify` than the official Spotify app.

## Target Design

### Principle: daemon owns playback truth

The TUI should not be responsible for reconciling playback vs queue vs device
truth. The CLI should not compute relative seek from a stale cached read. The
daemon should maintain an authoritative best-known player model and expose it
with freshness metadata.

### PlaybackClock

Add a daemon-owned `PlaybackClock` with this conceptual shape:

```rust
struct PlaybackClock {
    item: Option<MediaItem>,
    device: Option<Device>,
    is_playing: bool,
    base_progress_ms: u64,
    base_instant: Instant,
    provider_timestamp_ms: Option<u64>,
    sampled_at_ms: i64,
    shuffle: bool,
    repeat: String,
    source: PlaybackStateSource,
}

enum PlaybackStateSource {
    PlayerEvent,
    CommandResult,
    WebApiPoll,
    Cache,
    RecentFallback,
}
```

`current_progress_ms(now)`:

- if not playing: return `base_progress_ms`
- if playing: return `base_progress_ms + (now - base_instant)`
- clamp to track duration when available

The clock should be updated by:

1. `PlayerEvent` from embedded/local backend
2. command results from `actions::execute`
3. Web API poll reconciliation
4. cache/recent fallback at startup only

### Freshness metadata

Extend protocol/core playback state with enough metadata for clients to reason
about staleness:

```rust
pub struct Playback {
    pub item: Option<MediaItem>,
    pub device: Option<Device>,
    pub is_playing: bool,
    pub progress_ms: u64,
    pub shuffle: bool,
    pub repeat: String,
    pub sampled_at_ms: Option<i64>,
    pub provider_timestamp_ms: Option<i64>,
    pub source: Option<PlaybackStateSourceData>,
}
```

If adding fields directly to `spotuify_core::Playback` is too broad for one
patch, add a protocol wrapper first:

```rust
pub struct PlaybackEnvelope {
    pub playback: Playback,
    pub sampled_at_ms: i64,
    pub source: PlaybackStateSourceData,
}
```

Prefer the direct core fields if existing snapshots/tests can be updated with a
small diff. Prefer the wrapper if blast radius is high.

### Canonical now-playing source for TUI

The TUI should derive display state from one canonical object:

```rust
struct NowPlayingView {
    item: Option<MediaItem>,
    playback: Playback,
    queue_current: Option<MediaItem>,
    source: PlaybackStateSourceData,
    mismatches: Vec<NowPlayingMismatch>,
}
```

Rules:

- bottom player title uses daemon playback item
- queue rail can show queue current, but mismatches are logged
- queue fullscreen must not combine queue item with playback progress unless
  URIs match
- cover art is keyed by active playback item image URL
- lyrics are displayed only when `lyrics_track_uri == active_playback_uri`
- volume uses active playback device first, with devices cache used only to fill
  missing volume for the same active device id

### Relative seek moves into daemon

Protocol should support relative seek explicitly:

```rust
PlaybackCommand::SeekRelative { offset_ms: i64 }
PlaybackCommand::SeekAbsolute { position_ms: u64 }
```

If protocol churn must be minimized, keep existing `Seek { position_ms }` and add
a new `Request::PlaybackSeekRelative { offset_ms }`. But the cleaner model is to
make seek mode part of `PlaybackCommand`.

Daemon computes relative targets from `PlaybackClock`, not a client-side cached
read.

### Command confirmation model

Current two-stage mutation receipts are useful, but player controls need sharper
semantics in CLI/TUI copy:

- accepted: daemon accepted command and queued/started execution
- applied: local player event or command result updated clock
- confirmed: Web API reconciliation matched expected state
- failed: Spotify/backend returned error

The CLI can default to accepted for speed, but should provide a `--wait` or
`--wait-confirmed` path for scripts.

## Observability Plan

Use existing `tracing`, `tracing-subscriber`, and `tracing-appender`. Do not add a
new logging stack.

### Logging configuration cleanup

1. Make service files use `SPOTUIFY_LOG`, or make daemon logging honor
   `RUST_LOG` as fallback.
2. Decide whether to honor `SPOTUIFY_LOG_DIR` in active `log_path()` or remove
   the stale protocol path.
3. Keep JSON logs available through:

```sh
SPOTUIFY_LOG_FORMAT=json
SPOTUIFY_LOG=spotuify=trace,info
```

### IPC request spans

Add a span around daemon request handling:

Fields:

- `request_id`
- `request_kind`
- `category`
- `source`
- `duration_ms`
- `outcome`
- `error_kind`
- `receipt_id` when applicable

Do not log full payloads by default. Avoid search text, tokens, auth headers,
raw response bodies, or full file paths when unnecessary.

Example JSON event:

```json
{
  "level": "debug",
  "target": "spotuify_daemon::ipc",
  "request_id": 42,
  "request_kind": "playback-command",
  "command": "pause",
  "source": "tui",
  "duration_ms": 3,
  "outcome": "accepted"
}
```

### Spotify/API spans

Existing Spotify analytics events record API completion. For debug logs, add
short structured spans around player-sensitive calls:

- method
- redacted endpoint class
- priority
- duration
- status
- retry count
- rate-limit wait
- error class

For playback control, include:

- command kind
- target device id if available
- pre/post playback URI
- pre/post progress
- pre/post is_playing
- pre/post volume

### Daemon refresh spans

Add tracing to:

- `PlaybackGet`
- `QueueGet`
- `DevicesList`
- `spawn_playback_refresh`
- `spawn_queue_refresh`
- `spawn_devices_refresh`

Fields:

- cache hit/miss
- cache age
- cached playback URI
- cached queue current URI
- fetched playback URI
- fetched queue current URI
- fetched active device id
- fetched volume
- duration
- whether result was applied or dropped by mutation seq

Warning conditions:

- playback URI and queue current URI disagree after both are fresh
- refreshed playback is older than current clock source
- refresh failed after `PlaybackChanged`
- queue refresh says current URI differs from playback clock

### TUI spans

Add debug/trace logs to:

- `fetch_refresh`
- `apply_refresh`
- `merge_playback`
- `fetch_refresh_cover`
- `fetch_refresh_lyrics`
- `apply_daemon_event`

Fields:

- `refresh_id`
- per-read durations
- playback URI/progress/source
- queue current URI/count
- active device id/volume
- current cover URL vs incoming cover URL
- lyrics URI vs active playback URI
- mismatch warnings

Sample warnings:

- `tui_now_playing_mismatch`
- `tui_cover_stale_cleared`
- `tui_lyrics_stale_dropped`
- `tui_progress_reanchored`
- `tui_queue_progress_uri_mismatch`

### Visualizer diagnostics

Add or improve logs/status for:

- configured source
- active source
- enabled
- playing
- sink available
- loopback compiled
- loopback device name
- target fps
- frame count
- dropped frames
- last frame age
- peak
- hint

TUI should show a concise status when active source is none:

- "No PCM source. Use embedded backend or explicit loopback."

## Implementation Plan

### Phase 1: Observability and no-behavior-change diagnostics

Goal: Make the current failure measurable without changing playback behavior.

Changes:

- Add IPC request span.
- Add refresh spans for playback, queue, devices.
- Add TUI refresh/apply/mismatch logs.
- Add visualizer status logs and last-frame age.
- Fix logging env drift.

Acceptance criteria:

- `SPOTUIFY_LOG_FORMAT=json SPOTUIFY_LOG=spotuify=trace,info spotuify status`
  writes machine-readable request and refresh logs.
- `spotuify logs tail 100 --format json` can show:
  - IPC request kind
  - playback cache age
  - queue cache age
  - spawned refresh result
  - TUI mismatch warnings
- No live playback mutation is required to test this phase.

Suggested tests:

- log path env behavior
- JSON log tail pass-through/wrapping
- IPC request span emits kind/duration/outcome
- refresh failure logs error instead of silent return

### Phase 2: Persist post-command results before events

Goal: Remove the obvious stale event path.

Changes:

- In daemon playback command body, after `execute_with_device_recovery()`:
  - if `result.playback` exists, persist it
  - update playback clock if present
  - if `result.queue` exists, persist it
  - if `result.devices` exists, persist them
  - then emit `PlaybackChanged`
- Add trace fields comparing command result vs previous cached state.
- Warn if command result is missing after a command where refresh is expected.

Acceptance criteria:

- After fake pause/resume/seek/volume, `PlaybackGet` returns updated cached state
  immediately after the daemon event.
- TUI no longer has to wait for an unrelated background refresh to see command
  result state.

Suggested tests:

- daemon handler test: playback command persists returned playback before event
- daemon handler test: returned queue is persisted for queue-affecting command
- fake TUI test: `PlaybackChanged` followed by refresh applies new state

### Phase 3: PlaybackClock

Goal: Move current progress derivation to daemon.

Changes:

- Add `PlaybackClock` to daemon state.
- Initialize it from latest playback cache on startup.
- Update it from:
  - player events
  - post-command playback result
  - accepted seek command
  - Web API poll
- `PlaybackGet` should return `clock.snapshot()` when clock exists.
- Store writes should persist the sampled snapshot, but reads should not be the
  only way to get current progress.

Acceptance criteria:

- Progress advances on repeated `PlaybackGet` calls while playing even without
  new Web API polls.
- Progress stops while paused.
- Seek re-anchors progress immediately.
- Large Web API drift re-anchors and logs reason.

Suggested tests:

- `clock_advances_while_playing`
- `clock_does_not_advance_while_paused`
- `clock_seek_reanchors`
- `clock_track_change_resets_progress`
- `clock_web_api_drift_above_threshold_reanchors`

### Phase 4: Protocol freshness metadata

Goal: Make stale state explicit to clients and scripts.

Changes:

- Add source/sample fields to playback response or wrapper.
- Add source/sample fields to queue response or wrapper.
- Update CLI JSON output to include freshness metadata.
- Keep human/table output concise.

Acceptance criteria:

- `spotuify status --format json` includes playback source and sample time.
- Existing table output still works.
- JSON consumers can tell cache/recent fallback from player-event truth.

Compatibility note:

If changing `Playback` JSON directly is too risky, add additive optional fields
with serde defaults or use a protocol wrapper while retaining old output for
table/human formats.

### Phase 5: Relative seek in daemon

Goal: Stop computing relative seek from stale client state.

Changes:

- Add relative seek command/request.
- Parse CLI `+15s` / `-30s` into a relative request.
- Daemon computes target from `PlaybackClock`.
- Absolute seeks continue to send absolute position.
- TUI mouse seek remains absolute.

Acceptance criteria:

- `spotuify seek +15s` uses daemon-derived current progress.
- If no current track exists, daemon returns a clear error.
- If track duration exists, target is clamped to 0..duration.

Suggested tests:

- relative seek while playing uses elapsed clock
- relative seek while paused uses fixed clock
- negative relative seek clamps at zero
- relative seek without active item errors

### Phase 6: TUI canonical now-playing

Goal: Stop rendering mixed-track UI.

Changes:

- Add helper to derive canonical active playback URI and item.
- Bottom player uses playback item only, with explicit last-played fallback only
  when playback source is recent fallback or no active playback exists.
- Queue fullscreen uses playback item/progress for hero unless queue current URI
  matches.
- Queue rail can still show queue current, but mismatch is logged and visually
  treated as queue metadata, not player truth.
- Clear stale cover as soon as active art URL changes.
- Drop lyrics if returned URI differs from active playback URI.
- Re-anchor local TUI progress when incoming daemon snapshot includes seek/drift
  or a source newer than local state.

Acceptance criteria:

- Bottom title, cover, progress, and lyrics all refer to the same URI.
- Queue mismatch no longer causes progress from one track to be displayed beside
  title from another.
- Failed cover fetch shows current-track fallback art, not previous real cover.
- Lyrics for stale URI are not displayed.

Suggested tests:

- queue fullscreen does not mix queue URI and playback progress
- cover is cleared on art URL change before fetch success
- stale lyrics result is ignored
- volume update applies to active device only

### Phase 7: Visualizer truth and UX

Goal: Make "why no visualizer" obvious and make live source activation testable.

Changes:

- TUI reads daemon viz diagnostics on refresh and applies configured/active
  source instead of relying on local config only.
- Status line/player view shows active source when visualizer is enabled.
- If active source is none, show the daemon hint.
- If source is sink and backend is not embedded, hint says embedded is required.
- If source is loopback and macOS has no loopback device, hint says BlackHole or
  explicit setup is required.
- Add last-frame age to status/logs.

Acceptance criteria:

- `spotuify viz status --format json` explains enabled/configured/active source.
- TUI explains flat visualizer without user needing logs.
- Toggling visualizer emits source-change event and diagnostics update.

### Phase 8: Local player event priority

Goal: Make `spotuify-hume` faster than Web API polling when local playback is
owned by `spotuify`.

Changes:

- Prefer embedded librespot backend for `spotuify-hume` once stable on macOS.
- Feed `PlayerEvent` into `PlaybackClock`.
- Treat Web API poll as reconciliation only when local events are healthy.
- Poll Web API on:
  - startup
  - reconnect/session disconnect
  - device transfer to non-local device
  - no player event heartbeat for threshold
  - slow reconciliation interval

Acceptance criteria:

- Local play/pause/seek/track-change updates daemon clock from player event in
  under 100ms in fake/event tests.
- Web API polling no longer drives normal progress for local embedded playback.

## Live Investigation Protocol

The original request asked for static analysis, traces, CLI driving, log
monitoring, then user-performed TUI actions while logs are monitored. Do this
only after Phase 1 and Phase 2 are implemented.

### Read-only baseline

Use real state, but no playback mutations:

```sh
SPOTUIFY_LOG_FORMAT=json \
SPOTUIFY_LOG=spotuify=trace,info \
spotuify daemon restart

spotuify logs path
spotuify daemon status --format json
spotuify status --format json
spotuify queue --format json
spotuify devices --format json
spotuify viz status --format json
spotuify doctor --format json
spotuify logs tail 200 --format json
```

Record:

- playback URI
- queue current URI
- active device id/name
- device volume
- progress
- playback source/cache age
- queue source/cache age
- visualizer active source
- last frame age

### Controlled CLI mutation baseline

Use only reversible commands first:

```sh
before="$(spotuify status --format json)"
spotuify pause --format json
spotuify status --format json
spotuify resume --format json
spotuify status --format json
```

For volume, capture and restore:

```sh
spotuify status --format json
spotuify volume 45 --format json
spotuify status --format json
# restore previous volume from captured status
```

For seek, use small offsets:

```sh
spotuify seek +5s --format json
spotuify status --format json
spotuify seek -5s --format json
spotuify status --format json
```

Only run `next`/`previous` with user confirmation because it changes what is
playing.

### TUI session with user actions

Tail logs in one terminal:

```sh
spotuify logs tail 500 --follow --format json
```

Ask the user to perform:

1. open TUI and wait 10 seconds
2. press play/pause twice
3. seek forward/back
4. change volume
5. open queue rail
6. open lyrics rail
7. toggle visualizer
8. if comfortable, press next once

For each action, verify:

- TUI input log appears immediately
- daemon IPC accepted event appears
- Spotify/backend command start appears
- command result or player event appears
- playback clock updates
- TUI applies matching URI/progress/volume
- no stale cover/lyrics mismatch remains

## Test Plan

### Unit tests

Daemon:

- playback command persists `CommandResult.playback`
- playback command persists `CommandResult.queue`
- playback command persists `CommandResult.devices`
- `PlaybackClock` progress derivation
- stale Web API poll does not overwrite newer mutation/player-event state
- relative seek target uses daemon clock

TUI:

- stale cover cleared on art URL change
- stale lyrics dropped when URI mismatches
- queue fullscreen avoids mixed URI/progress display
- progress re-anchors on seek/drift
- volume display updates active device only

Protocol/output:

- playback freshness fields serialize with defaults
- status JSON includes source/sample metadata
- table output remains stable enough for humans

Logging:

- IPC span includes request kind/duration/outcome
- slow request warning emits after threshold
- logs tail JSON pass-through still works

### Smoke tests

Use fake provider by default:

```sh
scripts/smoke.sh
```

Add fake-specific smoke coverage for:

- `status --format json`
- `queue --format json`
- `devices --format json`
- `viz status --format json`
- pause/resume/seek/volume traces if fake provider supports safe mutation

### Live checks

Live checks are opt-in because they can affect real playback:

```sh
SPOTUIFY_LIVE_API=1 scripts/smoke.sh
```

Playback mutation checks must remain separate and explicit. Do not add default
agent smoke checks that repeatedly call Spotify live mutation APIs.

## Rollout Order

Recommended order:

1. Observability only.
2. Persist command results before events.
3. PlaybackClock.
4. Freshness metadata.
5. Relative seek daemon-side.
6. TUI canonical now-playing cleanup.
7. Visualizer diagnostics.
8. Embedded/player-event priority.

Why this order:

- Observability first prevents another blind rewrite.
- Persisting command results is a small, high-confidence bug fix.
- PlaybackClock addresses the core progress/lyrics drift.
- TUI cleanup is safer after daemon truth is better.
- Visualizer depends on knowing whether playback is local/embedded and whether a
  PCM source exists.

## Risks And Mitigations

### Risk: breaking JSON output consumers

Mitigation:

- Add optional fields with serde defaults.
- Keep existing core fields.
- Prefer additive metadata.
- Update snapshots.

### Risk: making TUI slower by waiting for coherent state

Mitigation:

- Do not block render on Spotify.
- Use daemon clock snapshots.
- Keep cover/lyrics asynchronous.
- Clear stale assets immediately and fill when ready.

### Risk: over-polling Spotify

Mitigation:

- Do not solve lag by polling faster.
- Use local clock extrapolation.
- Use local player events where available.
- Respect `Retry-After`.
- Use burst polling only after mutation and stop when expected state appears.

### Risk: local player event stream unavailable

Mitigation:

- Keep Web API fallback.
- Mark playback source as `web-api-poll` or `cache`.
- Surface degraded state in logs/doctor.

### Risk: visualizer policy ambiguity

Mitigation:

- Keep immediate work to diagnostics/source status.
- Do not ship beat-synced or synchronized visual media claims without policy
  review.
- Prefer local diagnostic equalizer framing.

## Acceptance Criteria

The player repair should be considered successful when:

- `spotuify status --format json` reports playback source and sample time.
- After pause/resume/seek/volume, daemon state updates before or at the same
  time as `PlaybackChanged`.
- TUI title, cover, progress, volume, and lyrics use one active playback URI.
- Queue can disagree temporarily, but mismatch is logged and not rendered as a
  combined player truth.
- Relative seek lands relative to the real daemon-derived position.
- Visualizer status clearly explains whether bars can move and why.
- Live logs can answer where latency came from:
  - TUI input
  - IPC accept
  - mutation lock wait
  - token/auth file
  - rate limiter
  - Spotify round trip
  - command result
  - player event
  - cache persist
  - TUI apply

## Source Links

Spotify official docs:

- Current playback:
  https://developer.spotify.com/documentation/web-api/reference/get-information-about-the-users-current-playback
- Queue:
  https://developer.spotify.com/documentation/web-api/reference/get-queue
- Playback mutations:
  https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback
- Devices:
  https://developer.spotify.com/documentation/web-api/reference/get-a-users-available-devices
- Rate limits:
  https://developer.spotify.com/documentation/web-api/concepts/rate-limits
- Web Playback SDK:
  https://developer.spotify.com/documentation/web-playback-sdk
- Web API changes:
  https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api
- Developer policy:
  https://developer.spotify.com/policy

Other primary/reference docs:

- librespot player source:
  https://docs.rs/librespot-playback/latest/src/librespot_playback/player.rs.html
- librespot player config:
  https://docs.rs/librespot-playback/latest/librespot_playback/config/struct.PlayerConfig.html
- spotifyd MPRIS:
  https://docs.spotifyd.rs/advanced/dbus.html
- MPRIS Player interface:
  https://specifications.freedesktop.org/mpris/latest/Player_Interface.html

Local project docs:

- `docs/blueprint/07-player.md`
- `docs/implementation/09-phase-6-sync-hardening.md`
- `docs/implementation/12-phase-9-librespot-embed.md`
- `docs/implementation/20-phase-17-audio-visualization.md`
