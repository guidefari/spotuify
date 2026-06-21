# Spotuify macOS Client — Surface Reference

> A complete inventory of every affordance, screen, sub-screen, setting, IPC
> request, persisted key, and backing CLI command in the SwiftUI macOS client
> under `clients/macos/`. Written as a porting spec: if you wanted to rebuild
> this client in a different language, every input and output you need is
> listed here.

Source of truth: `clients/macos/Sources/` (74 Swift files, ~9,900 LoC) and
`src/main.rs` (the Rust CLI binary that every surface ultimately calls). The
macOS client is a *view* of the local `spotuify` daemon — it owns no playback
state and produces no audio. The daemon owns everything; the client renders
daemon state and forwards user intent as IPC requests.

Target: macOS 26 (Tahoe) · Swift 6 · Xcode 26.3+ · XcodeGen-managed project.

---

## 1. High-level architecture

```text
                                    ┌────────────────────────────────────────┐
                                    │  spotuify daemon (Rust)                │
                                    │                                        │
                                    │  ┌──────────┐ ┌──────────┐ ┌────────┐  │
                                    │  │ Spotify  │ │ SQLite   │ │Tantivy │  │
                                    │  │ Web API  │ │ cache    │ │search  │  │
                                    │  └────┬─────┘ └────┬─────┘ └───┬────┘  │
                                    │       └────────┐  │           │       │
                                    │                ▼  ▼           ▼       │
                                    │  ┌──────────────────────────────────┐  │
                                    │  │  IPC server (length-delim JSON,  │  │
                                    │  │  AF_UNIX socket) + event bus     │  │
                                    │  └────────────────┬─────────────────┘  │
                                    └───────────────────┼────────────────────┘
                                                        │
                          ┌─────────────────────────────┼─────────────────────────┐
                          │                             │                         │
                          ▼                             ▼                         ▼
                 ┌────────────────┐           ┌────────────────┐         ┌────────────────┐
                 │   CLI (Rust)   │           │   TUI (Rust)   │         │  macOS client  │
                 │   `spotuify …` │           │   `spotuify`   │         │  (Swift /      │
                 │   src/main.rs  │           │   (ratatui)    │         │   SwiftUI)     │
                 └────────────────┘           └────────────────┘         └────────────────┘
```

The macOS client is one of three client surfaces over the same daemon. Every
affordance in the client maps to either a daemon IPC request (the canonical
path) or a `spotuify <subcommand>` invocation (the few config / Homebrew /
debug paths that the CLI implements but the daemon's IPC does not expose
yet).

### 1.1 App-internal layering

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Sources/Spotuify/                       SwiftUI app target                   │
│                                                                              │
│   SpotuifyApp.swift ─────────── declares 4 window scenes (player,            │
│   │                              mini-player, settings) + 1 MenuBarExtra    │
│   ├── Views/Shell/             AppShell, Sidebar, DaemonGateView, Navigator │
│   ├── Views/Player/            NowPlayingView, NowPlayingBar, DeviceMenu,   │
│   │                           SeekBar, VolumeControl                       │
│   ├── Views/Search/            SearchView                                    │
│   ├── Views/Library/           LikedSongsView, AlbumsView, ArtistsView      │
│   ├── Views/Playlists/         PlaylistsView, PlaylistDetailView            │
│   ├── Views/Podcasts/          PodcastsView                                  │
│   ├── Views/History/           HistoryView, SessionDetailView               │
│   ├── Views/Reminders/         RemindersView, ReminderPickerView            │
│   ├── Views/Lyrics/            LyricsView                                    │
│   ├── Views/Devices/           DevicesView                                   │
│   ├── Views/Settings/          SettingsView (9 panes)                       │
│   ├── Views/Detail/            MediaDetailViews (5 detail types)            │
│   ├── Views/MiniPlayer/        MiniPlayerView (3 sizes)                     │
│   ├── Views/MenuBar/           MenuBarView                                   │
│   ├── Views/Common/            AsyncCoverImage, MediaRow, MediaItemMenu,    │
│   │                           TrackListView, CollectionView, Visualizer,   │
│   │                           SkeletonPlaceholder                          │
│   ├── System/                  KeyboardController, SystemMediaController,    │
│   │                           ReminderNotificationScheduler, AppRelaunch,  │
│   │                           CoverArtCache, TerminalLauncher              │
│   └── Theme/                   Theme tokens (12 files)                      │
├──────────────────────────────────────────────────────────────────────────────┤
│ Sources/SpotuifyKit/                     Framework target (testable)         │
│                                                                              │
│   ├── Models/                  DaemonRequest, DaemonEvent, Response,        │
│   │                           Domain, Reminders, Wire                       │
│   ├── Networking/              DaemonConnection, IPCSocket, FrameCodec,     │
│   │                           SocketPath, ConnectionState,                 │
│   │                           DaemonLauncher, DaemonControl, CLIRunner     │
│   ├── Stores/                  AppModel, PlayerStore, SearchStore,         │
│   │                           LibraryStore, LyricsStore, RemindersStore,   │
│   │                           PodcastsStore, ConfigStore, VizStore        │
│   └── Services/                AppUpdater (DMG-based self-update)           │
└──────────────────────────────────────────────────────────────────────────────┘
```

The split is deliberate: `SpotuifyKit` is a framework that compiles without
SwiftUI. It owns all IPC, wire encoding, and state. `Spotuify` is the app
target and contains every view. `SpotuifyKitTests` exercises the wire format
and the live-daemon round-trip without booting any UI.

---

## 2. Window / scene / lifecycle model

`SpotuifyApp.swift` declares four SwiftUI scenes. Each is a single reusable
`Window` (or `MenuBarExtra`), never a `WindowGroup` — re-invoking the
`openWindow(id:)` focuses the existing window instead of opening a new one.

| Scene id | Title | Default size | Root view | Notes |
|---|---|---|---|---|
| `"player"` | Spotuify | 980×720 | `RootView()` → `AppShell()` when daemon is ready, else `DaemonGateView(readiness:)` | `.windowResizability(.contentSize)`. Hosts `.commands { … }` (Settings, Playback, Go, Check for Updates, Mini Player). |
| `"mini-player"` | Mini Player | 320×380 | `MiniPlayerView()` (3 sizes: 320×380 / 320×132 / 360×64) | Floating panel, `.canJoinAllSpaces`, transparent titlebar, hidden zoom/miniaturize. |
| `"settings"` | Settings | 760×540 | `SettingsView()` (9 panes) | `Window` not `Settings` scene — keeps the titlebar chrome consistent with the main window. |
| (none) | menubar | 320 pt popover | `MenuBarView()` | `MenuBarExtra("Spotuify", systemImage: "music.note") { … }` with `.menuBarExtraStyle(.window)`. |

`AppDelegate.applicationShouldTerminateAfterLastWindowClosed` returns `false`
— closing the player window keeps the process alive. The menubar item stays
resident so the user can reopen the window. `RootView` gates the player UI on
`model.readiness` and shows `DaemonGateView` until the daemon is present and
running a compatible protocol version.

---

## 3. The IPC contract (read this before porting)

This is the single thing a porting client has to implement. The daemon speaks
**length-delimited JSON** over an **AF_UNIX** stream socket. Every frame is:

```
┌────────────────────────────────────────────────────────────┐
│ 4 bytes  │  UTF-8 JSON  (≤ 16 MB)                          │
│ big-end  │                                                │
│ UInt32   │                                                │
└────────────────────────────────────────────────────────────┘
```

This is the same wire format as `tokio_util::LengthDelimitedCodec` with
`length_field_length(4)`. See `Sources/SpotuifyKit/Networking/FrameCodec.swift`
and `Sources/SpotuifyKit/Networking/IPCSocket.swift` for the reference
implementation.

**Socket path resolution** (`Sources/SpotuifyKit/Networking/SocketPath.swift`):
`SPOTUIFY_SOCKET` → `SPOTUIFY_RUNTIME_DIR/daemon.sock` →
`~/Library/Application Support/<instance>/daemon.sock` where `<instance>` is
`SPOTUIFY_INSTANCE` or `"spotuify"` (installed) / `"spotuify-dev"` (cargo).

**Protocol version gate** (`SpotuifyKit.ipcProtocolVersion = 6`): if the
running daemon advertises a lower `protocol_version`, the app shows
`DaemonGateView(.incompatible(found, required, version))` with brew upgrade
instructions.

### 3.1 Request envelope

```json
{ "type": "Request", "cmd": "<kebab>", …args }
```

Outbound: `Sources/SpotuifyKit/Models/DaemonRequest.swift` is the canonical
list. The Rust side is `spotuify_protocol::Request`; the parity test
`Tests/SpotuifyKitTests/ProtocolParityTests.swift` walks `DaemonRequest.allSamples`
and diffs the encoded `cmd` strings against a fixture.

#### Full request roster with backing CLI commands

The macOS client emits these `cmd` strings. The rightmost column is the
`spotuify <subcommand>` that exercises the same daemon path, per the project
rule "every feature must be exposed via the CLI."

| DaemonRequest | Wire `cmd` | Used by | CLI equivalent |
|---|---|---|---|
| `.ping` | `ping` | (heartbeat / hand-rolled probes) | `spotuify ping` |
| `.getDaemonStatus` | `get-daemon-status` | supervisor on connect | `spotuify daemon status` |
| `.subscribeEvents` | `subscribe-events` | once on connect | (one-shot internal) |
| `.clientSeed` | `client-seed` | once on connect | (one-shot internal) |
| `.playbackGet` | `playback-get` | event-driven refresh | `spotuify status` |
| `.queueGet` | `queue-get` | event-driven refresh | `spotuify queue` |
| `.devicesList` | `devices-list` | event-driven refresh | `spotuify devices` |
| `.playlistsList` | `playlists-list` | `PlaylistsView` / `LibraryStore` | `spotuify playlists` |
| `.recentlyPlayed` | `recently-played` | (seed consumers) | `spotuify recently-played` |
| `.libraryList(limit)` | `library-list` | `AlbumsView` / `LibraryStore` | `spotuify library` (albums) |
| `.playbackCommand(.pause)` | `playback-command` (body `pause`) | `Playback menu` / `AppModel.togglePlayPause` | `spotuify pause` |
| `.playbackCommand(.resume)` | …body `resume` | `AppModel.togglePlayPause` | `spotuify resume` |
| `.playbackCommand(.toggle)` | …body `toggle` | (one-off) | `spotuify toggle` |
| `.playbackCommand(.next)` | …body `next` | transport | `spotuify next` |
| `.playbackCommand(.previous)` | …body `previous` | transport | `spotuify previous` |
| `.playbackCommand(.playURI(uri))` | …body `play-uri { uri }` | every `Play` affordance | `spotuify play <uri>` |
| `.playbackCommand(.seek(positionMs))` | …body `seek { position_ms }` | `SeekBar`, `LyricsView` | `spotuify seek <ms>` |
| `.playbackCommand(.seekRelative(offsetMs))` | …body `seek-relative { offset_ms }` | (reserved) | `spotuify seek +/-<ms>` |
| `.playbackCommand(.volume(percent))` | …body `volume { volume_percent }` | `VolumeControl`, ⌘↑/⌘↓ | `spotuify volume <pct>` |
| `.playbackCommand(.shuffle(state))` | …body `shuffle { state }` | `Toggle Shuffle` | `spotuify shuffle on/off` |
| `.playbackCommand(.repeatMode(mode))` | …body `repeat { state }` | `Cycle Repeat` | `spotuify repeat off/context/track` |
| `.deviceTransfer(device)` | `device-transfer { device }` | `DeviceMenu`, `DevicesView` | `spotuify transfer <device>` |
| `.search(query, scope, source, limit, kinds, sort)` | `search { … }` | `SearchStore` | `spotuify search <q>` |
| `.searchStream(query, scope, source, version)` | `search-stream { … }` | (streaming) | `spotuify search --pages 2` |
| `.searchPage(query, kind, offset, version)` | `search-page { … }` | (paged) | `spotuify search-page` |
| `.queueAdd(uri)` | `queue-add { uri }` | `MediaRow` plus button, `MediaItemMenu` | `spotuify queue add <uri>` |
| `.queueAddMany(uris)` | `queue-add-many { uris }` | `model.queueAll` | `spotuify queue add --many` |
| `.savedTracks(limit, offset)` | `saved-tracks { limit, offset }` | `LikedSongsView` | `spotuify library tracks` |
| `.savedShows(limit)` | `saved-shows { limit }` | `PodcastsView` | (read via `SavedShows` listing) |
| `.showEpisodes(show, limit, offset)` | `show-episodes { … }` | `ShowDetailView`, `PodcastsView` | `spotuify show episodes <show>` |
| `.albumTracks(album)` | `album-tracks { album }` | `AlbumDetailView` | `spotuify album tracks <album>` |
| `.artistAlbums(artist)` | `artist-albums { artist }` | `ArtistDetailView` | `spotuify artist albums <artist>` |
| `.followedArtists(limit)` | `followed-artists { limit }` | `ArtistsView` | `spotuify followed` |
| `.artistFollow(artist)` | `artist-follow { artist }` | `MediaItemMenu`, `ArtistDetailView` | `spotuify follow <uri>` |
| `.artistUnfollow(artist)` | `artist-unfollow { artist }` | `ArtistDetailView` | `spotuify unfollow <uri>` |
| `.listenSessions(limit)` | `listen-sessions { limit }` | `HistoryView` | `spotuify history` |
| `.playlistTracks(playlist, wait)` | `playlist-tracks { … }` | `PlaylistDetailView` / `PlaylistItemDetailView` | `spotuify playlist tracks <id>` |
| `.playlistAddItems(playlist, uris)` | `playlist-add-items { … }` | (n/a in views, used by `playlist add` CLI) | `spotuify playlist add <id> <uri…>` |
| `.librarySave(uri?, current)` | `library-save { uri, current }` | `AlbumDetailView`, `NowPlayingLikeButton` | `spotuify like` (current) or `spotuify save <uri>` |
| `.libraryUnsave(uri)` | `library-unsave { uri }` | `AlbumDetailView`, `NowPlayingLikeButton` | `spotuify unlike <uri>` |
| `.lyricsGet(trackURI?, forceRefresh)` | `lyrics-get { … }` | `LyricsStore` | `spotuify lyrics` |
| `.lyricsOffsetSet(trackURI, offsetMs)` | `lyrics-offset-set { … }` | (n/a in views yet) | `spotuify lyrics offset` |
| `.coverArt(url)` | `cover-art { url }` | `CoverArtCache` (image path) | (internal cache read) |
| `.setVizEnabled(Bool)` | `set-viz-enabled { enabled }` | (n/a in macOS — TUI only) | `spotuify viz enable/disable` |
| `.reminderCreate(uri, anchorAtMs, recurrence, tz, message?)` | `reminder-create { … }` | `ReminderPickerView` | `spotuify reminder create` |
| `.remindersList(includeInactive)` | `reminders-list { … }` | `RemindersStore` | `spotuify reminder list` |
| `.reminderCancel(id)` | `reminder-cancel { id }` | `RemindersView` | `spotuify reminder cancel <id>` |
| `.notificationsList(includeArchived)` | `notifications-list { … }` | `RemindersStore` | `spotuify notification list` |
| `.notificationAct(id, action, snoozeUntilMs?)` | `notification-act { … }` | `RemindersView`, notification actions | `spotuify notification act` |
| `.checkUpdate(force)` | `check-update { force }` | `SettingsView`, `AppModel` | `spotuify update check` |
| `.episodeFeed(limit, sort, refresh)` | `episode-feed { … }` | `PodcastsStore` | `spotuify episode-feed` |
| `.shutdown` | `shutdown` | (n/a in macOS — debug) | `spotuify daemon shutdown` |
| `.getDoctorReport` | `get-doctor-report` | (n/a in macOS — debug) | `spotuify doctor` |
| `.reindex` | `reindex` | (n/a in macOS — debug) | `spotuify reindex` |
| `.cacheStatus` | `cache-status` | (n/a in macOS — debug) | `spotuify cache status` |
| `.logsTail(lines)` | `logs-tail { lines }` | (n/a in macOS — debug) | `spotuify logs tail` |
| `.sync(target)` | `sync { target }` | (n/a in macOS — debug) | `spotuify sync` |
| `.image(url)` | `image { url }` | (raw cover fallback) | (internal) |
| `.reconnect` | `reconnect` | Settings "Daemon" pane | `spotuify reconnect` |
| `.setAudioOutput(device?)` | `set-audio-output { device? }` | Settings → Audio Output | `spotuify audio-output` |
| `.reload` | `reload` | triggered by `ConfigStore` after a non-player config change | `spotuify reload` |
| `.reloadAuth` | `reload-auth` | (n/a in macOS) | `spotuify auth reload` |
| `.webApiToken(force)` | `web-api-token { force }` | (debug probe) | `spotuify auth bearer` |
| `.searchCachePrune(olderThanMs?)` | `search-cache-prune { … }` | (n/a in macOS) | `spotuify search cache prune` |
| `.playlistCreate(name, description?, uris)` | `playlist-create { … }` | (n/a in views — CLI/TUI/MCP) | `spotuify playlist create` |
| `.playlistRemoveItems(playlist, uris)` | `playlist-remove-items { … }` | (n/a in views) | `spotuify playlist remove` |
| `.playlistSetImage(playlist, imageBase64)` | `playlist-set-image { … }` | (n/a in views) | `spotuify playlist set-image` |
| `.playlistUnfollow(playlist)` | `playlist-unfollow { playlist }` | (n/a in views) | `spotuify playlist unfollow` |
| `.getVizStatus` | `get-viz-status` | (n/a in macOS) | `spotuify viz status` |
| `.setVizSource(kind)` | `set-viz-source { kind }` | (n/a in macOS — TUI) | `spotuify viz source` |
| `.setVizFocus(focused)` | `set-viz-focus { focused }` | `RootView` (vote) | (per-client, not exposed) |
| `.opsLog(limit, sinceMs?, source?)` | `ops-log { … }` | (n/a in views yet) | `spotuify ops log` |
| `.opsShow(operationId, withDiff)` | `ops-show { … }` | (n/a in views yet) | `spotuify ops show` |
| `.opsUndo(operationId?, dryRun, force, bulkSinceMs?)` | `ops-undo { … }` | (n/a in views yet) | `spotuify ops undo` |
| `.opsRedo(operationId?)` | `ops-redo { … }` | (n/a in views yet) | `spotuify ops redo` |
| `.analyticsTop(kind, sinceWindow, limit)` | `analytics-top { … }` | (n/a in views yet) | `spotuify analytics top` |
| `.analyticsHabits(window, sinceMs?)` | `analytics-habits { … }` | (n/a in views yet) | `spotuify analytics habits` |
| `.analyticsSearch(mode, limit)` | `analytics-search { … }` | (n/a in views yet) | `spotuify analytics search` |
| `.analyticsRediscovery(gapDays)` | `analytics-rediscovery { … }` | (n/a in views yet) | `spotuify analytics rediscovery` |
| `.analyticsRebuild(sinceMs?)` | `analytics-rebuild { … }` | (n/a in views yet) | `spotuify analytics rebuild` |
| `.analyticsPrune(apply)` | `analytics-prune { … }` | (n/a in views yet) | `spotuify analytics prune` |
| `.relatedArtists(artist)` | `related-artists { artist }` | (n/a in views yet) | `spotuify related` |
| `.radioStart(seedUri, dryRun)` | `radio-start { … }` | (n/a in views yet) | `spotuify radio` |

### 3.2 Response envelope

```json
{ "type": "Response", "id": <u64>, "Ok": { "data": <ResponseData> } }
{ "type": "Response", "id": <u64>, "Error": { "kind": …, "message": …, "code"?: …, "retryable"?: bool } }
```

`ResponseData` kinds (the Swift side is
`Sources/SpotuifyKit/Models/Response.swift`):

| `kind` string | Swift case | Carries |
|---|---|---|
| `pong` | `.pong` | — |
| `daemon-status` | `.daemonStatus(DaemonStatus)` | `running`, `protocol_version`, `daemon_version?`, `daemon_pid?` |
| `playback` | `.playback(Playback)` | full `Playback` snapshot |
| `devices` | `.devices([Device])` | `[]Device` |
| `queue` | `.queue(Queue)` | `Queue` (current + upcoming) |
| `client-seed` | `.clientSeed(ClientSeed)` | `playback + queue + devices + recent` |
| `search-results` | `.searchResults([MediaItem])` | one-shot search results |
| `search-started` | `.searchStarted(query, version)` | streaming search ack |
| `playlists` | `.playlists([Playlist])` | `[]Playlist` |
| `media-items` | `.mediaItems([MediaItem])` | generic media list |
| `listen-sessions` | `.listenSessions([ListenSession])` | session-grouped history |
| `lyrics` | `.lyrics(SyncedLyrics?, offsetMs)` | optional lyrics + user offset |
| `cover-art` | `.coverArt(path, cacheHit, bytes, fetchedAtMs?)` | on-disk cover path |
| `mutation` | `.mutation(CommandReceipt)` | `{ ok, action, message }` |
| `ack` | `.ack(message)` | plain ack |
| `web-api-token` | `.webApiToken(String?)` | bearer token or null |
| `reminders` | `.reminders([Reminder])` | `[]Reminder` |
| `notifications` | `.notifications([ReminderNotification])` | `[]ReminderNotification` |
| `reminder-created` | `.reminderCreated(Reminder)` | the newly created reminder |
| `update-status` | `.updateStatus(UpdateStatus)` | `update_available`, `current_version`, `latest_version?`, `release_url?`, `upgrade` (method/command/url), `checked_at_ms?` |
| (unknown) | `.unknown(kind)` | future-proofing |

### 3.3 Event envelope

```json
{ "type": "Event", "event": "<kebab>", …payload }
```

Events the daemon broadcasts to subscribed clients
(`Sources/SpotuifyKit/Models/DaemonEvent.swift`). The Swift decoder falls
back to `.unknown(event:)` for any event the app doesn't render, which
future-proofs the client.

| `event` | Swift case | Carries | Observed by | UI effect |
|---|---|---|---|---|
| `playback-changed` | `.playbackChanged` | `action`, `playback?` | `AppModel.handle` | every surface reads `model.player` (now playing, transport, seek bar, lyrics position) |
| `queue-changed` | `.queueChanged` | `action`, `uris`, `queue?` | `AppModel.handle` | `NowPlayingBar`, `NowPlayingView`, `NowPlayingQueue` |
| `devices-changed` | `.devicesChanged` | `action`, `devices?` | `AppModel.handle` | `DeviceMenu`, `VolumeControl`, `DevicesView` |
| `playlists-changed` | `.playlistsChanged` | `action`, `playlist?` | `LibraryStore` | `PlaylistsView` refetches |
| `library-changed` | `.libraryChanged` | `action`, `uris` | `LibraryStore` | refetches liked / albums / followed artists (saved-shows still half-finished) |
| `search-updated` | `.searchUpdated` | `query`, `count` | (parsed) | reserved for streaming search progress |
| `search-page` | `.searchPage` | `query`, `kind`, `offset`, `version`, `items` | (parsed) | streaming search chunk |
| `search-complete` | `.searchComplete` | `query`, `version` | (parsed) | streaming search done |
| `search-failed` | `.searchFailed` | `query`, `version`, `kind?`, `offset?`, `message` | (parsed) | search error toast |
| `event-stream-lagged` | `.eventStreamLagged` | `skipped` | `AppModel.handle` | triggers reseed |
| `rate-limited` | `.rateLimited` | `retry_after_secs`, `scope` | `AppModel.handle` | banner ("Rate limited — retrying in Ns") |
| `auth-error` | `.authError` | `kind` | `AppModel.handle` | banner ("Sign-in needed — run `spotuify login`") |
| `player-ready` | `.playerReady` | `device_id?`, `name?` | `AppModel.handle` | clears banner |
| `player-degraded` | `.playerDegraded` | `reason` | (parsed) | reserved |
| `premium-required` | `.premiumRequired` | — | `AppModel.handle` | banner ("Spotify Premium required for playback") |
| `session-disconnected` | `.sessionDisconnected` | `reason` | (parsed) | reserved |
| `player-failed` | `.playerFailed` | `reason`, `restarts` | `AppModel.handle` | banner + suggested `spotuify reconnect` |
| `spectrum-frame` | `.spectrumFrame` | `bands[12]`, `peak`, `timestamp_ms` | `AppModel.handle` → `VizStore` | drives `VisualizerView` (bars / circular / wave) |
| `config-reloaded` | `.configReloaded` | — | (parsed) | (no UI effect in macOS today) |
| `shutdown-requested` | `.shutdownRequested` | — | (parsed) | reserved |
| `reminder-due` | `.reminderDue` | `notification` | `RemindersStore`, `AppModel` (banner), `ReminderNotificationScheduler` | OS notification + due-inbox sheet |
| `reminders-changed` | `.remindersChanged` | `action` | `RemindersStore`, `ReminderNotificationScheduler` | re-sync OS notifications, refetch |
| `update-available` | `.updateAvailable` | `latest_version`, `release_url?`, `upgrade` | `AppModel.handle` | "newer release available" banner + one-click install |

### 3.4 Correlation, timeouts, reconnect

`DaemonConnection` is an `actor` (`Sources/SpotuifyKit/Networking/DaemonConnection.swift`).
Every outbound request carries a monotonically increasing `id`. A pending
continuation is keyed by id; responses resume it. A timeout task (default
30 s, 8 s for `get-daemon-status`, 10 s for `client-seed`) cancels the
continuation with `DaemonConnectionError.timeout`. If the socket closes
mid-request, every pending continuation is failed with
`DaemonConnectionError.disconnected` and the supervisor reconnects with
exponential backoff (250 ms × 2^attempt, capped at 10 s).

The connect → subscribe → seed sequence happens once per connect:

```text
AppModel.runSupervisor()
  └─ ensureRunning(path)            ← DaemonLauncher / DaemonControl
  └─ connection.connect(path)        ← AF_UNIX socket
  └─ fetchDaemonStatus()            ← get-daemon-status (8s timeout)
  └─ if protocolVersion < required → readiness = .incompatible → block
  └─ subscribeEvents()              ← subscribe-events (one-shot)
  └─ reseed()                       ← client-seed (10s timeout)
  └─ checkUpdate()                  ← check-update (cached)
  └─ reminders.loadAll()            ← reminders-list + notifications-list
  └─ onRemindersReady?()            ← one-shot, drives ReminderNotificationScheduler
  └─ if openNotifications non-empty && not yet shown → presentDueInbox = true
  └─ await connection.waitUntilClosed()  ← suspend; on close, retry
```

`RootView.onDisappear` (the player window closing, e.g. via menubar-only mode)
withdraws the visualizer focus vote so a windowless app doesn't pin the
daemon at full spectrum-frame rate.

---

## 4. State model (the AppModel + 8 stores)

`Sources/SpotuifyKit/Stores/AppModel.swift` is the only entry point for
view-side mutations. Views never call `connection.request` directly (with
one exception: `AlbumDetailView`'s optimistic save/unsave). Everything else
goes through an `AppModel` helper that wraps the request in a fire-and-forget
Task, or a store that owns its request lifecycle.

| Store | What it holds | Owns requests to | Subscribed to events |
|---|---|---|---|
| `AppModel` | `connectionState`, `readiness`, `recent`, `banner`, `availableUpdate`, `toast`, `presentDueInbox`; supervisors; event router | (everything) | (everything) |
| `PlayerStore` | `playback`, `queue`, `devices`, `displayProgressMs` (interpolated by a 250 ms ticker between `sampled_at_ms` events) | (none — applied only) | `playbackChanged`, `queueChanged`, `devicesChanged` (via `AppModel`) |
| `SearchStore` | `query`, `typeFilter`, `sort`, `source`, `results`, `isSearching`, `errorMessage` | `.search` | — |
| `PodcastsStore` | `mode` (shows/episodes), `query`, `source`, `episodeSort`, `episodeFeed`, `spotifyResults`, loading flags | `.episodeFeed`, `.search(scope:.show/.episode)` | — |
| `LibraryStore` | `playlists`, `likedSongs`, `savedAlbums`, `savedShows`, `followedArtists`, `historySessions`, `playlistTracks` | `.playlistsList`, `.savedTracks`, `.libraryList`, `.savedShows`, `.followedArtists`, `.listenSessions`, `.playlistTracks` | `playlistsChanged`, `libraryChanged` |
| `ConfigStore` | `values: [String:String]`, `audioOutputs: [String]` | **CLI only** (not IPC): `spotuify config show`, `spotuify audio-outputs`, `spotuify config set`, `spotuify reload`/`reconnect` | — |
| `LyricsStore` | `lyrics`, `offsetMs`, `loading`, `loadedURI` | `.lyricsGet` | — |
| `RemindersStore` | `reminders`, `notifications`, `loading` | `.remindersList`, `.notificationsList` | `reminderDue`, `remindersChanged` |
| `VizStore` | `bands[12]`, `peak` | (none) | `spectrumFrame` (via `AppModel`) |
| `AppUpdater` | `phase` (`idle / downloading / verifying / installing / installed(url) / failed(msg)`) | (network: GitHub DMG + `hdiutil` + `ditto`) | — |

`ConfigStore` is the deliberate exception: settings edits in `SettingsView`
go through `spotuify config set <key> <value>` (the CLI), not through a
daemon IPC, because the daemon reads its config from disk on `reload` /
`reconnect`. The macOS side mirrors that boundary so a manual `vim` on the
config file still applies after the next `reload`.

`AppModel.send(_:)` is fire-and-forget for transport mutations. The view
calls it; the daemon processes the request and emits a `playbackChanged`
event; the event router applies the new state to `PlayerStore`; the view
re-renders from `PlayerStore`. The `player.isPlaying` check before sending
`pause` vs `resume` is local and authoritative for the "what would I send
if I tapped this now" question — the daemon is the final source of truth.

The optimistic-UI path (e.g. `NowPlayingLikeButton`, the Save toggle in
`AlbumDetailView`) flips a local override, dispatches the mutation, and
drops the override when the authoritative state catches up. This is the
"daemon owns state, but the user gets instant feedback" rule from `AGENTS.md`.

---

## 5. Sidebar destinations (top-level screens)

`Sources/Spotuify/Views/Shell/Destination.swift` defines ten destinations in
sidebar order (matches `Navigator.numbered`, the ⌘1…⌘9/⌘0 order). The
`Queue` is *not* a top-level destination — it lives in the Now Playing mode
pill and as a global right-hand rail from the footer.

| # | Destination | View file | View struct | Primary content |
|---|---|---|---|---|
| 1 | Now Playing | `Player/NowPlayingView.swift` | `NowPlayingView` | Immersive album stage with mode pill (Artwork / Visualizer / Lyrics / Up next), minimize toggle, transport + metadata over a palette scrim |
| 2 | Search | `Search/SearchView.swift` | `SearchView` | `TextField` + source picker (Spotify / Library) + filter chips (All / Track / Artist / Album / Playlist / Show / Episode) + sort menu + grouped results |
| 3 | Liked Songs | `Library/LibraryView.swift` | `LikedSongsView` | `CollectionHeader` (Play / Shuffle / Queue All) + `TrackListView` of `LibraryStore.likedSongs` |
| 4 | Albums | `Library/LibraryView.swift` | `AlbumsView` | `EditorialPageHeader` + `CollectionView` (grid/list) of `LibraryStore.savedAlbums` |
| 5 | Artists | `Library/LibraryView.swift` | `ArtistsView` | `CollectionView` of `LibraryStore.followedArtists`; tap → `ArtistDetailView` |
| 6 | Podcasts | `Podcasts/PodcastsView.swift` | `PodcastsView` | `Picker` (Shows / Episodes) + Library/Spotify source + sort + filtered list |
| 7 | Playlists | `Playlists/PlaylistsView.swift` | `PlaylistsView` | `CollectionView` of `LibraryStore.playlists`; tap → `PlaylistDetailView` |
| 8 | History | `History/HistoryView.swift` | `HistoryView` | `Picker` (Recent / Sessions); flat `MediaRow` list or session cards → `SessionDetailView` |
| 9 | Notifications | `Reminders/RemindersView.swift` | `RemindersView` | "Inbox" + "Scheduled" sections with `NotificationRow` / `ReminderRow` |
| 10 | Devices | `Devices/DevicesView.swift` | `DevicesView` | `EditorialPageHeader` + per-device `DeviceRow` (tap to transfer) |

The bottom of the sidebar shows a `Connected / Connecting / Reconnecting (n)…
/ Daemon offline / Starting…` badge bound to `model.connectionState`. The
"spotuify" wordmark at the top is `Fraunces 22pt` and uses the active
tint.

### 5.1 Now Playing

File: `Sources/Spotuify/Views/Player/NowPlayingView.swift` (588 lines, the
single most complex view in the app).

**Modes** (`NowPlayingMode`): `artwork`, `visualizer`, `lyrics`, `queue`.
The mode pill is a single glass capsule that swaps the middle slot.
`@AppStorage("nowPlayingMode")` persists the choice.

**Visualizer style pill** (only in `.visualizer` mode):
`@AppStorage("vizStyle")` cycles between `bars`, `circular`, `wave`. The
spectrum data comes from `VizStore` (12 bands), driven by the daemon's
`spectrumFrame` events (high-rate; gated by `setVizFocus`).

**Minimize / full art**: `@AppStorage("nowPlayingMinimized")`. When true, the
mode pill, transport, and metadata disappear; tapping the full-bleed cover
restores them. The footer `NowPlayingBar` is suppressed on the Now Playing
page unless the stage is minimized, so the user always has a way to control
playback.

**Metadata**: `item.albumNavItem` is a `NavigationLink` to
`AlbumDetailView`; `item.artistNavItems` is one `NavigationLink` per
`ArtistRef` to `ArtistDetailView`. Both rely on the
`mediaDetailDestinations()` extension in `Detail/MediaDetailViews.swift`.

**Transport row**: `DeviceMenu` (left) · shuffle / prev / play-pause /
next / repeat (center, in a single glass capsule) · `VolumeControl` (right).
`TransportButton` is a stateless themed button from `Theme/Theme.swift`.

**Lyrics stage**: shares `LyricsView` (in `Lyrics/LyricsView.swift`) with
the dedicated Lyrics destination. Tap a line → `model.seek(toMs:
line.startMs)`.

**Up-next stage**: shares `NowPlayingQueue` with the global right-hand rail.
Tap an upcoming row → `model.play(uri:)`.

**Like / heart button** (above metadata): `NowPlayingLikeButton` —
optimistic local state (`@State optimistic: Bool?`), `model.likeCurrent()`,
`.symbolEffect(.bounce)` on every tap, drop the override when
`item.inLibrary` or `item.uri` changes.

### 5.2 Search

File: `Sources/Spotuify/Views/Search/SearchView.swift`.

| Affordance | Behavior |
|---|---|
| `TextField` ("Search songs, artists, albums, playlists…") | On submit: `model.search.runSearch()`. On change (debounced 350 ms in `SearchStore.scheduleSearch`): same. |
| X clear button | Resets `search.query = ""` |
| Source `Picker` (Spotify / Library) | `model.search.setSource(.spotify / .local / .hybrid)` |
| Filter chips (`SearchFilterChip`): All + per-`MediaKind` | `model.search.toggleFilter(.track / .artist / .album / .playlist / .show / .episode)` |
| Sort `Menu` (Relevance / Name / Duration / Artist / Date) | `model.search.setSort(...)` |
| Results | `model.search.grouped` — section headers by `MediaKind.sectionTitle`; tracks/episodes as `MediaRow`; others as `NavigationLink(value: item)` to detail destinations |
| Empty / error states | `ContentUnavailableView("Search", …)`, `ContentUnavailableView("Search failed", …)` |
| Skeleton while loading | `SkeletonRows` |

`SearchStore.runSearch` issues `Request::Search { query, scope:.all, source,
limit:40, kinds, sort }`. The `kinds` parameter is derived from the
`typeFilter` set. There is no in-view event subscription; results are
pushed into `search.results` by the response handler.

### 5.3 Liked Songs

File: `Sources/Spotuify/Views/Library/LibraryView.swift` (`LikedSongsView`).

| Affordance | Behavior |
|---|---|
| `CollectionHeader` Play (borderedProminent) | `model.playAll(uris: likedSongs.map(\.uri))` |
| Shuffle | `model.shufflePlay(uris:)` |
| Queue All | `model.queueAll(uris:)` |
| `TrackListView` rows | `MediaRow` with `LibraryStore.likedSongs` (loaded via `LibraryStore.loadLiked()` → `.savedTracks(limit:1000, offset:0)`) |
| `LayoutToggle` | toggles `likedLayout` `AppStorage` between grid and list |

`.onAppear` triggers `loadLiked()`; `.libraryChanged` event causes a forced
reload.

### 5.4 Albums

File: `Sources/Spotuify/Views/Library/LibraryView.swift` (`AlbumsView`).

`CollectionView(items: savedAlbums, storageKey: "albumsLayout")` over
`LibraryStore.savedAlbums` (loaded via `.libraryList(limit:200)`). Tap →
`AlbumDetailView`. Long-press / `⋯` menu → `MediaItemMenu`.

### 5.5 Artists

File: `Sources/Spotuify/Views/Library/LibraryView.swift` (`ArtistsView`).

`CollectionView(items: followedArtists, storageKey: "artistsLayout",
minTile:150, maxTile:190)` over `LibraryStore.followedArtists` (loaded
via `.followedArtists(limit:500)`). Tap → `ArtistDetailView`.

`ArtistDetailView` (in `Detail/MediaDetailViews.swift`):
`DetailHeader(artworkIsCircle: true)`, Follow / Following toggle, "All / In
Library" `Picker`, sections (`Albums / Singles & EPs / Compilations /
Appears On / Other`) grouped by `MediaItem.albumGroup`. Issues
`.artistAlbums(artist: artist.uri)` on appear; optimistic follow/unfollow
via `model.followArtist(uri:)` / `model.unfollowArtist(uri:)`.

### 5.6 Podcasts

File: `Sources/Spotuify/Views/Podcasts/PodcastsView.swift`.

| Affordance | Behavior |
|---|---|
| Mode `Picker` (Shows / Episodes) | `model.podcasts.setMode(...)` |
| Source `Picker` (Library / Spotify) | `model.podcasts.setSource(...)` |
| Filter `TextField` ("Search Spotify…" / "Filter…") | `model.podcasts.scheduleSearch()` (350 ms debounce) |
| Sort `Menu` (in Episodes mode: Newest / Oldest / Duration / Title / Show) | `model.podcasts.setEpisodeSort(...)` |
| Shows grid | `CollectionView(items: podcasts.shows(libraryShows:), storageKey: "podcastsLayout")` — `model.library.savedShows` for Library mode; `spotifyResults` for Spotify mode |
| Episodes list | `MediaRow(item: episode, detailed: true)` over `model.podcasts.episodes` |
| Skeleton / empty / error | `SkeletonTiles`, `SkeletonRows`, `ContentUnavailableView` |

`PodcastsStore.loadEpisodes(refresh:)` issues `.episodeFeed(limit:200, sort,
refresh)`. Spotify search for shows/episodes issues
`.search(query, scope:.show or .episode, source:.spotify, limit:40,
kinds:nil, sort:.date or nil)`.

`ShowDetailView` (in `Detail/MediaDetailViews.swift`): `DetailHeader` +
"Unplayed only" `Toggle` + "Newest first / Oldest first" `Picker` +
`MediaRow` list. Issues `.showEpisodes(show: show.uri, limit:50, offset:0)`.

### 5.7 Playlists

File: `Sources/Spotuify/Views/Playlists/PlaylistsView.swift`.

`CollectionView(items: LibraryStore.playlists, storageKey: "playlistsLayout")`.
Each item is a `MediaItem` synthesised from a `Playlist` (uri
`spotify:playlist:<id>`, kind `.playlist`). Tap → `PlaylistDetailView`.

`PlaylistDetailView` (in the same file): 120×120 cover, title, "N tracks ·
owner", Play / Shuffle / Add to Queue, then a `TrackListView`. Issues
`.playlistTracks(playlist: playlist.id, wait:true)`.

### 5.8 History

File: `Sources/Spotuify/Views/History/HistoryView.swift`.

`@AppStorage("historySessionMode")` toggles between "Recent" (flat
chronological `MediaRow` list) and "Sessions" (cards → `SessionDetailView`).
Data comes from `LibraryStore.loadHistory()` → `.listenSessions(limit:50)`.

`SessionDetailView` is just a `DetailHeader` + `TrackListView` over the
session's tracks.

### 5.9 Notifications

File: `Sources/Spotuify/Views/Reminders/RemindersView.swift`.

Two sections: **Inbox** (open notifications, count badge) and **Scheduled**
(active reminders, count badge).

| Affordance | Behavior |
|---|---|
| Inbox row `Play` button | `model.actNotification(id:, action:"play")` |
| Inbox row `Queue` button | `model.actNotification(id:, action:"queue")` |
| Inbox row `Dismiss` | `model.actNotification(id:, action:"dismiss")` |
| Inbox row `Menu` (Snooze 1h / 4h / Tomorrow) | `model.snoozeNotification(id:, for: …)` → `actNotification(action:"snooze", snoozeUntilMs: …)` |
| Scheduled row `Cancel` | `model.cancelReminder(id:)` → `.reminderCancel` |
| "Show all" in `DueRemindersSheet` | `navigator.selection = .notifications` + `dismiss()` |

The "Remind me…" entry on `MediaItemMenu` opens `ReminderPickerView` (a
sheet). The due-inbox sheet (`DueRemindersSheet`) is presented automatically
once on connect when `model.reminders.openNotifications` is non-empty.

### 5.10 Devices

File: `Sources/Spotuify/Views/Devices/DevicesView.swift`.

`DevicesView` lists `model.player.devices`. Per row (`DeviceRow`): icon
(kind-specific SF Symbol), name, kind label, volume badge; tap →
`model.transfer(to: device)`. The active device is highlighted. Empty state
is `ContentUnavailableView("No devices", …)`.

`DeviceMenu` (in `Player/DeviceMenu.swift`) is a more compact version of the
same picker — used in `NowPlayingBar`, `NowPlayingView`, and `MenuBarView`.

---

## 6. Settings (`SettingsView`, 9 panes)

File: `Sources/Spotuify/Views/Settings/SettingsView.swift`. Pane switcher
in a `NavigationSplitView` sidebar (`Pane` enum). `model.config.set(key,
value)` writes through `ConfigStore` → `CLIRunner.run(["config", "set",
key, value])`, then `CLIRunner.run(["reconnect"])` (for `player.*` keys) or
`["reload"]` (for everything else). `CLIRunner.run(["config", "path"])`
returns the config file path for "Open config file" in the Daemon pane.

| Pane | Field | Backing CLI / key |
|---|---|---|
| **Account** | `TextField` for `client_id` | `spotuify config set client_id <v>` |
| | `SecureField` for `client_secret` (commits on submit, redacts display) | `spotuify config set client_secret <v>` |
| | `TextField` for `redirect_uri` | `spotuify config set redirect_uri <v>` |
| **Appearance** | `Picker` (radio) of `ThemePreference`: `.system` / `.light` / `.dark` / `.adaptive` (with tile UI) | `@AppStorage("themePreference")` (not daemon config) |
| **Playback** | `TextField` for `player.backend` | `spotuify config set player.backend <v>` (triggers reconnect) |
| | `Picker` (96 / 160 / 320 kbps) for `player.bitrate` | `spotuify config set player.bitrate <v>` |
| | `TextField` for `player.device_name` | `spotuify config set player.device_name <v>` |
| | `Toggle` for `player.normalization` | `spotuify config set player.normalization <v>` |
| | Int `TextField` for `player.audio_cache_mib` | `spotuify config set player.audio_cache_mib <v>` |
| | `TextField` for `player.event_hook` | `spotuify config set player.event_hook <v>` |
| **Audio Output** | `Picker` (System default + `model.config.audioOutputs`) for `player.audio_output_device` | `spotuify config set player.audio_output_device <v>` + `spotuify audio-outputs` (enumerated) |
| **Notifications** | `Toggle` for `notifications.enabled` | `spotuify config set notifications.enabled <v>` |
| | `Toggle` for `notifications.on_track_change` | `spotuify config set notifications.on_track_change <v>` |
| | `Toggle` for `notifications.on_pause` | `spotuify config set notifications.on_pause <v>` |
| | `Toggle` for `notifications.on_resume` | `spotuify config set notifications.on_resume <v>` |
| | `Toggle` for `notifications.on_skip` | `spotuify config set notifications.on_skip <v>` |
| | `Toggle` for `notifications.on_error` | `spotuify config set notifications.on_error <v>` |
| | `TextField` for `notifications.summary` | `spotuify config set notifications.summary <v>` |
| | `TextField` for `notifications.body` | `spotuify config set notifications.body <v>` |
| **Privacy & Cache** | `TextField` for `analytics.hook_command` | `spotuify config set analytics.hook_command <v>` |
| | Int `TextField` for `analytics.hook_timeout_ms` | `spotuify config set analytics.hook_timeout_ms <v>` |
| | Int `TextField` for `cache.cover_cache_mb` | `spotuify config set cache.cover_cache_mb <v>` |
| | Int `TextField` for `cache.cover_cache_ttl_days` | `spotuify config set cache.cover_cache_ttl_days <v>` |
| **Updates** | `LabeledContent` for app version (from `CFBundleShortVersionString`) | (read-only) |
| | `Toggle` "Check for updates automatically" (`@AppStorage("autoCheckUpdates")`) | (gates `model.checkUpdate(...)` flow) |
| | `Button` "Check Now" | `model.checkUpdate(force: true)` → `.check-update` |
| | (when `availableUpdate != nil`) Status / Relaunch / Retry / Open releases page / Update Now | `model.installAvailableUpdate()` → `AppUpdater.install(version:)` (downloads DMG, verifies SHA-256, mounts, swaps bundle) |
| | (when `availableUpdate != nil`) "Or via terminal" command | `availableUpdate.command` from daemon (brew / cargo) |
| **Daemon** | Connection status row (color dot + label) | `model.connectionState` |
| | Socket path (monospaced, selectable) | `model.socketPath` (`SocketPath.resolve()`) |
| | `Button` "Reconnect" | `model.forceReconnect()` → close + supervisor |
| | `Button` "Open config file" | `CLIRunner.run(["config","path"])` → `NSWorkspace.shared.activateFileViewerSelecting` |
| **About** | App name / version / releases link | (read-only) |

`Pane` enum order in the sidebar: `.account, .appearance, .playback, .audio,
.notifications, .privacy, .updates, .daemon, .about`.

---

## 7. Always-visible chrome

### 7.1 `NowPlayingBar` (footer of every non-NowPlaying page)

File: `Sources/Spotuify/Views/Player/NowPlayingBar.swift`.

Left: track cell (cover + name/subtitle). Center: thin seek bar with
elapsed / total time. Right: shuffle / prev / play-pause / next / repeat
(transport row), trailing cluster (Lyrics toggle, Up-next toggle, time
label, `DeviceMenu(showsActiveName:false)`, `VolumeControl`). The two
right-side toggles write to `@AppStorage("globalSidePanel")` →
`AppShell` then shows `GlobalSidePanel(panel:)` (340 pt) on the right.

### 7.2 Global right-hand rail

`GlobalSidePanel` (in `AppShell.swift`):
- `.queue` → `NowPlayingQueue` (shared with the Now Playing stage)
- `.lyrics` → `LyricsView` (shared with the Lyrics mode)
- close button writes `globalPanelRaw = GlobalPanel.none.rawValue`

### 7.3 Banners / toasts (`AppShell`)

| Element | Trigger | Affordance |
|---|---|---|
| Top banner | `model.banner` is non-nil (rate-limited, premium, auth, player-failed, reminder-due) | single line + × |
| Update banner | `autoCheckUpdates` on, no banner, `availableUpdate != nil` | phase-driven: `ProgressView` while downloading/verifying/installing; "Relaunch" when installed; "Open releases page" + "Retry" on failed; "Update Now" when idle. × to dismiss. |
| Toast (bottom) | `model.toast` is non-nil (added to queue, added to library, following, unfollowed, …) | one-line, auto-dismisses after 1.8 s |
| Due-inbox sheet | one-shot on connect when `model.reminders.openNotifications` is non-empty | `DueRemindersSheet` with "Show all" / "Done" |

---

## 8. App menu / keyboard shortcuts

Defined in `Sources/Spotuify/SpotuifyApp.swift`'s `.commands { … }`.

| Group | Menu item | Shortcut | Action |
|---|---|---|---|
| `.appSettings` | Settings… | ⌘, | `openWindow(id: "settings")` |
| `.appSettings` | Check for Updates… | — | `model.checkUpdate(force: true)` + `openWindow(id: "settings")` |
| `.windowArrangement` | Mini Player | ⌘⇧M | `openWindow(id: "mini-player")` |
| `Playback` | Play / Pause | Space (bare, yields to text fields) | `model.togglePlayPause()` |
| `Playback` | Next | ⌘→ | `model.next()` |
| `Playback` | Previous | ⌘← | `model.previous()` |
| `Playback` | Volume Up | ⌘↑ | `model.setVolume(current + 5)` |
| `Playback` | Volume Down | ⌘↓ | `model.setVolume(current − 5)` |
| `Playback` | Toggle Shuffle | ⌘⇧S | `model.toggleShuffle()` |
| `Playback` | Cycle Repeat | ⌘⇧R | `model.cycleRepeat()` |
| `Go` | (per `Navigator.numbered[i]`) | ⌘1…⌘9, ⌘0 | `navigator.selection = dest` |

Order of `Navigator.numbered`: `nowPlaying, search, likedSongs, albums,
artists, podcasts, playlists, history, devices` (Notifications is reachable
from the sidebar but not the Go menu).

`KeyboardController` (`Sources/Spotuify/System/KeyboardController.swift`)
hooks bare Space at the app level so it works even when no menu is
focused; ⌘/⌥/⌃ chords pass through to the system. It checks the key
window's first responder and skips Space when a text field is focused.

---

## 9. Sheets / modals / popovers

| Triggered by | Type | View | Dismissed by |
|---|---|---|---|
| `MediaItemMenu` "Remind me…" (per row) | `.sheet` | `ReminderPickerView(item:)` | "Cancel" / "Set Reminder" → `model.createReminder(...)` |
| `AppModel` supervisor (one-shot on connect) | `.sheet` on `AppShell` | `DueRemindersSheet` | "Done" / "Show all" → `navigator.selection = .notifications` |
| App menu / ⌘, | `Window` "settings" | `SettingsView` | standard window chrome |
| App menu / ⌘⇧M | `Window` "mini-player" | `MiniPlayerView` (3 sizes, always-on-top) | standard window chrome |
| Menubar click | `MenuBarExtra` popover (window style) | `MenuBarView` (320 pt) | click outside / explicit dismiss |
| `DeviceMenu` (now playing + footer + menubar) | `Menu` popup | (macOS menu) | click outside |
| `CollectionHeader` action buttons | (in-page) | (buttons) | tap |
| `MediaRow` `⋯` button / right-click | `Menu` (the `MediaItemMenu`) | (macOS menu) | click outside |
| Now Playing mode pill | (in-stage swap) | (mode switch) | n/a |
| Now Playing viz style pill | (in-stage swap) | (visualizer style) | n/a |
| Now Playing minimize / restore | (in-stage swap) | (minimize toggle) | n/a |
| Detail push (Album/Artist/Show/Playlist/Session) | `NavigationLink` | `AlbumDetailView` / `ArtistDetailView` / `ShowDetailView` / `PlaylistItemDetailView` / `SingleItemDetailView` / `SessionDetailView` | back button |

---

## 10. System integrations

| File | Purpose |
|---|---|
| `Sources/Spotuify/System/KeyboardController.swift` | `NSEvent.addLocalMonitorForEvents(matching: .keyDown)` — bare Space → `model.togglePlayPause()`. Suppresses when first responder is a text field. |
| `Sources/Spotuify/System/SystemMediaController.swift` | `MPRemoteCommandCenter` wiring (play/pause/next/prev/seek-position enabled, the rest disabled). Claims Now Playing via `MPNowPlayingInfoCenter`. Republishes on `model.player.playback / currentItem?.uri / isPlaying` change. |
| `Sources/Spotuify/System/ReminderNotificationScheduler.swift` | `UNUserNotificationCenter` delegate. Category `REMINDER` with actions `PLAY` / `QUEUE` / `SNOOZE` (1h) / `DISMISS`. `sync()` re-creates `UNCalendarNotificationTrigger`s per active reminder. Tapped actions route to `model.play(uri:)` / `model.actLatestNotification(reminderID:action:)`. |
| `Sources/Spotuify/System/AppRelaunch.swift` | `relaunch(from: URL)` — polls the old PID for up to 10 s, then `exec /usr/bin/open <url>` and `NSApp.terminate(nil)`. |
| `Sources/Spotuify/System/CoverArtCache.swift` | `static let shared`. `image(for urlString)` — NSCache → in-flight `Task` dedupe → `.coverArt(url:)` IPC → on-disk path → `NSImage(contentsOfFile:)` → `URLSession` fallback. |
| `Sources/Spotuify/System/TerminalLauncher.swift` | `run([String])` — opens Terminal.app running a small shell that pipes the commands to a fresh shell. |
| `Sources/SpotuifyKit/Networking/DaemonLauncher.swift` | `bundledBinaryPath()`, `resolveBinary()` (SPOTUIFY_BIN > `/opt/homebrew/bin` > `/usr/local/bin` > `~/.local/bin` > `~/.cargo/bin` > PATH > bundled), `installBundledCLIIfNeeded()` (copies bundled `spotuify` to `~/.local/bin`), `ensureRunning(socketPath:timeout:)` (probes, respects `intentional-stop` sentinel for 30 s, runs `spotuify daemon start`, polls). |
| `Sources/SpotuifyKit/Networking/DaemonControl.swift` | `startDaemon(socketPath:)` → `DaemonLauncher.ensureRunning`. `installViaBrew(socketPath:)` / `updateViaBrew(socketPath:)` (one-click Homebrew). `homebrewAvailable`. `brewUpdateCommands`. |
| `Sources/SpotuifyKit/Services/AppUpdater.swift` | One-click in-app update: download DMG from `https://github.com/planetaryescape/spotuify/releases/download/v{ver}/Spotuify-{ver}.dmg`, fetch `.sha256`, verify, `hdiutil attach -nobrowse -readonly -plist`, `ditto` the `.app` to a temp dir, swap into `Bundle.main.bundleURL` parent, expose `installed(url)` so the banner can show "Relaunch". |

---

## 11. Persisted settings (`@AppStorage` keys)

| Key | Default | Type | Set by |
|---|---|---|---|
| `themePreference` | `.system` | `ThemePreference` enum | Settings → Appearance (radio tiles) |
| `nowPlayingMode` | `.artwork` | `NowPlayingMode` enum | Now Playing mode pill |
| `nowPlayingMinimized` | `false` | `Bool` | Now Playing chevron / tap-to-restore |
| `vizStyle` | `.bars` | `VizStyle` enum | Now Playing viz style pill |
| `globalSidePanel` | `none.rawValue` | `GlobalPanel` enum | Now Playing footer Lyrics / Up-next toggles; `GlobalSidePanel` close |
| `autoCheckUpdates` | `true` | `Bool` | Settings → Updates toggle |
| `historySessionMode` | `false` | `Bool` (false=Recent, true=Sessions) | History picker |
| `miniSize` | `.full.rawValue` | `MiniSize` enum | Mini Player size button |
| `likedLayout` | `.grid` | `CollectionLayout` enum | Liked Songs `LayoutToggle` |
| `albumsLayout` | `.grid` | `CollectionLayout` enum | Albums `LayoutToggle` |
| `artistsLayout` | `.grid` | `CollectionLayout` enum | Artists `LayoutToggle` |
| `playlistsLayout` | `.grid` | `CollectionLayout` enum | Playlists `LayoutToggle` |
| `podcastsLayout` | `.grid` | `CollectionLayout` enum | Podcasts `LayoutToggle` |
| `trackListLayout` | `.list` | `CollectionLayout` enum | headerless `TrackListView` consumers (album/playlist/show/session detail) |

(All `CollectionLayout` keys go through `@propertyWrapper
CollectionLayoutStorage` so each surface has its own persisted value.)

Config keys that are *not* `@AppStorage` but live in the daemon's TOML config
(see Settings table above): `client_id`, `client_secret`, `redirect_uri`,
`player.backend`, `player.bitrate`, `player.device_name`,
`player.normalization`, `player.audio_cache_mib`, `player.event_hook`,
`player.audio_output_device`, `notifications.enabled`,
`notifications.on_track_change`, `notifications.on_pause`,
`notifications.on_resume`, `notifications.on_skip`,
`notifications.on_error`, `notifications.summary`, `notifications.body`,
`analytics.hook_command`, `analytics.hook_timeout_ms`,
`cache.cover_cache_mb`, `cache.cover_cache_ttl_days`.

---

## 12. The "If I tap this, what fires?" cheat sheet

A compact lookup of every view-side affordance, what it sends to the
daemon, and what its `spotuify` CLI equivalent is. (Most of these have
already been enumerated above; this is the single-page-ops summary.)

| Affordance | View | IPC | CLI |
|---|---|---|---|
| Tap play/pause | transport (everywhere) | `.playbackCommand(.resume / .pause)` | `spotuify resume` / `spotuify pause` |
| Tap next/prev | transport | `.playbackCommand(.next / .previous)` | `spotuify next` / `spotuify previous` |
| Drag volume | `VolumeControl` | `.playbackCommand(.volume(p))` | `spotuify volume <p>` |
| Tap shuffle | transport | `.playbackCommand(.shuffle(state))` | `spotuify shuffle on/off` |
| Tap repeat | transport | `.playbackCommand(.repeatMode(mode))` | `spotuify repeat off/context/track` |
| Seek (drag) | `SeekBar` | `.playbackCommand(.seek(positionMs))` | `spotuify seek <ms>` |
| Play a URI | `MediaRow` double-click / `MediaItemMenu` Play | `.playbackCommand(.playURI(uri))` | `spotuify play <uri>` |
| Add a URI to queue | `MediaRow` + button / `MediaItemMenu` | `.queueAdd(uri)` | `spotuify queue add <uri>` |
| Add many to queue | `CollectionHeader` Queue All | `.queueAddMany(uris)` | `spotuify queue add --many` |
| Like a track | `NowPlayingLikeButton` / `MediaItemMenu` | `.librarySave(uri, current:false)` or `.librarySave(uri:nil, current:true)` | `spotuify save <uri>` / `spotuify like` |
| Unlike a track | `NowPlayingLikeButton` / `MediaItemMenu` | `.libraryUnsave(uri)` | `spotuify unlike <uri>` |
| Save an album | `AlbumDetailView` Save toggle | `.librarySave(uri, current:false)` | `spotuify save <uri>` |
| Transfer device | `DeviceMenu` / `DevicesView` row | `.deviceTransfer(device)` | `spotuify transfer <device>` |
| Search (typed) | `SearchView` | `.search(query, scope, source, limit, kinds, sort)` | `spotuify search <q>` |
| Liked Songs page | `LikedSongsView` | `.savedTracks(limit:1000, offset:0)` | `spotuify library tracks` |
| Albums page | `AlbumsView` | `.libraryList(limit:200)` | `spotuify library` |
| Artists page | `ArtistsView` | `.followedArtists(limit:500)` | `spotuify followed` |
| Open artist → albums | `ArtistDetailView` | `.artistAlbums(artist)` | `spotuify artist albums <uri>` |
| Follow / unfollow artist | `ArtistDetailView` | `.artistFollow(uri)` / `.artistUnfollow(uri)` | `spotuify follow <uri>` / `spotuify unfollow <uri>` |
| Playlists page | `PlaylistsView` | `.playlistsList` | `spotuify playlists` |
| Open playlist | `PlaylistDetailView` | `.playlistTracks(playlist, wait:true)` | `spotuify playlist tracks <id>` |
| Podcasts → shows | `PodcastsView` (Library) | `.savedShows(limit:200)` | (read via saved-shows) |
| Podcasts → episodes | `PodcastsView` (Episodes) | `.episodeFeed(limit:200, sort, refresh)` | `spotuify episode-feed` |
| Open show → episodes | `ShowDetailView` | `.showEpisodes(show, limit:50, offset:0)` | `spotuify show episodes <show>` |
| Open album → tracks | `AlbumDetailView` | `.albumTracks(album)` | `spotuify album tracks <album>` |
| History page | `HistoryView` | `.listenSessions(limit:50)` | `spotuify history` |
| Lyrics | `LyricsView` | `.lyricsGet(trackURI)` | `spotuify lyrics` |
| Tap a lyrics line | `LyricsView` | `.playbackCommand(.seek(line.startMs))` | `spotuify seek <ms>` |
| Schedule reminder | `ReminderPickerView` "Set Reminder" | `.reminderCreate(uri, anchorAtMs, recurrence, tz, message?)` | `spotuify reminder create` |
| Cancel reminder | `RemindersView` Scheduled row | `.reminderCancel(id)` | `spotuify reminder cancel <id>` |
| Act on inbox notification | `RemindersView` row button | `.notificationAct(id, action, snoozeUntilMs?)` | `spotuify notification act` |
| Snooze inbox notification | `RemindersView` row menu | `.notificationAct(id, action:"snooze", snoozeUntilMs: …)` | (subcommand of act) |
| Viz focus vote | `RootView` on focus / blur | `.setVizFocus(focused)` | (per-client, not exposed) |
| Audio output change | Settings → Audio Output | daemon config + `set-audio-output` | `spotuify audio-output` |
| Reconnect | Settings → Daemon Reconnect | `.reconnect` | `spotuify reconnect` |
| Reload | (auto) after non-player config change | `.reload` | `spotuify reload` |
| Open config file | Settings → Daemon Open config file | `CLIRunner.run(["config","path"])` | `spotuify config path` |
| Check for updates | Settings → Updates Check Now | `.checkUpdate(force: true)` | `spotuify update check` |
| Update Now | Settings → Updates | (DMG download + `hdiutil` + `ditto` via `AppUpdater`) | (one-click; brew fallback surfaced as `availableUpdate.command`) |
| Install (one-click) | `DaemonGateView` "Install spotuify" | (Homebrew) | `brew install planetaryescape/spotuify/spotuify` |
| Update & Restart (one-click) | `DaemonGateView` "Update & Restart" | (Homebrew + restart) | `brew upgrade …` + `spotuify daemon restart` |
| Start daemon | `DaemonGateView` "Start spotuify" | `DaemonLauncher.ensureRunning` | `spotuify daemon start` |
| Reconnect (gate) | `DaemonGateView` "Retry" | close + supervisor reconnect | `spotuify reconnect` |

---

## 13. Theme / design system (high-level)

The 12 files under `Sources/Spotuify/Theme/` define the design tokens. A
porting client that doesn't reuse the same tokens can keep the structure:

- `Theme.swift` — `timeString(ms)`, spacing/font helpers
- `ThemeTokens.swift`, `RadiusTokens.swift`, `OpacityTokens.swift`,
  `ShadowTokens.swift`, `StatusTokens.swift` — chrome-level numeric tokens
- `AlbumStageTokens.swift` — the now-playing stage's text/scrim tokens
- `ArtworkTheme.swift`, `ArtworkPalette.swift` — `@Observable` palette
  derived from the current track's cover (background / accent / primary /
  secondary). `RootView` (or `AppShell`) calls `await theme.update(for:
  imageURL)` whenever the playing cover changes; `AsyncCoverImage` and
  the immersive views use `theme.palette` for tinting
- `ThemePreference.swift` — `.system` / `.light` / `.dark` / `.adaptive`
  (adaptive = artwork-driven wash)
- `EditorialFont.swift` — Fraunces (display) + a system sans (body);
  registered in `AppDelegate.applicationWillFinishLaunching`
- `ThemedView.swift` — the top-level `View` wrapper that applies the
  preferred color scheme and the system tint

`AppDelegate.swift` is 13 lines: register the font, and keep the app alive
after the last window closes.

---

## 14. App lifecycle / startup

```text
1. AppDelegate.applicationWillFinishLaunching
     └─ EditorialFont.register()
2. Window("player") task
     ├─ DaemonLauncher.installBundledCLIIfNeeded()   // copy bundled `spotuify` to ~/.local/bin if no install
     ├─ AppModel.start()                             // idempotent
     │     ├─ eventTask: AsyncStream consumer       // AppModel.handle
     │     └─ supervisor: runSupervisor()           // see §3.4
     ├─ SystemMediaController.shared.configure(model:)
     ├─ KeyboardController.shared.configure(model:)
     └─ ReminderNotificationScheduler.shared.configure(model:)
3. RootView gates on model.readiness
     ├─ .ready  → AppShell
     └─ else    → DaemonGateView (install / start / update / docs)
4. AppShell on appear: theme.update(for: currentItem?.imageURL)
5. Window("settings") task: model.config.load() + model.config.loadAudioOutputs()
6. Window("mini-player") task: model.start() (idempotent)
7. MenuBarExtra renders MenuBarView on demand
```

The 4 scenes can each be opened independently; `AppModel.start()` is
idempotent so multiple scenes booting the model in parallel is safe.

---

## 15. Porting checklist

A porting client in any other language needs to:

1. **Speak the wire format** (§3). One file. Length-delimited JSON over an
   AF_UNIX (or platform equivalent) socket. Max 16 MB per frame. Request →
   response correlation by `id`. Events are broadcast on the same socket
   after a `subscribe-events` call.
2. **Honor the protocol version gate.** Call `get-daemon-status` first; if
   `protocol_version < ipcProtocolVersion`, show a "daemon out of date"
   gate with brew/cargo upgrade commands.
3. **Model state as 9 stores** (`§4`). The daemon is the source of truth
   for everything user-visible; the client only renders. The one exception
   is optimistic local overrides (like, save) that drop when authoritative
   state catches up.
4. **Implement the 10 destinations** (`§5`) with the affordances listed.
   The detail destinations push via `NavigationLink` (or the platform
   equivalent) to Album / Artist / Show / Playlist / Session detail pages.
5. **Implement the 9 settings panes** (`§6`). The easiest path is to issue
   `spotuify config set <key> <value>` + `reconnect`/`reload` for each
   field; this skips needing the daemon to learn a `config-set` IPC.
6. **Wire the 14 system bridges** (`§10`). The only macOS-specific ones
   are the Now Playing claim, the key monitor, the OS notification
   scheduler, the relaunch helper, and the cover-art cache. Replace each
   with the platform equivalent.
7. **Persist the 14 `AppStorage` keys** (`§11`). All are per-client
   display state; none of them belong in the daemon's TOML config.
8. **Implement the keyboard shortcuts** (`§8`). `Space` (bare, yields to
   text fields), `⌘arrows` (next/prev/volume), `⌘⇧S/⌘⇧R` (shuffle/repeat),
   `⌘,` (settings), `⌘⇧M` (mini player), `⌘1…⌘9/⌘0` (Go).
9. **Drive the 25 event types** with at least the 15 the macOS app
   actually consumes (`§3.3`). Treat the rest as `.unknown` and ignore.
10. **Use the `spotuify` binary for first-run install / start / update**.
    One-click Homebrew install is part of the user experience. `TerminalLauncher`
    is the fallback for the cases where the GUI buttons fail.

### Things *not* in the macOS client (deliberately CLI/TUI/MCP-only)

- The operation log (`ops-log` / `ops-show` / `ops-undo` / `ops-redo`)
  has no UI; the CLI's `spotuify ops` and the MCP expose it.
- Analytics (`analytics-top` / `habits` / `search` / `rediscovery` /
  `rebuild` / `prune`) is CLI/TUI today.
- Playlist plan / agent workflows (`playlistCreate` /
  `playlist-plan` / `playlistSetImage` / `playlistUnfollow` / `radioStart`
  / `relatedArtists`) are CLI/TUI/MCP.
- The `search-stream` / `search-page` (paged, high-volume) flow is the
  TUI's. The macOS client uses the one-shot `search` and lets the
  daemon aggregate.
- `set-viz-enabled` / `set-viz-source` / `get-viz-status` are TUI-only.
  The macOS client only *votes* on `set-viz-focus` (so the daemon's
  spectrum broadcast is paused when no one is rendering it).
- `reindex`, `cache-status`, `logs-tail`, `shutdown`, `get-doctor-report`,
  `mpris` are debug surfaces reachable via CLI.

If a ported client is targeting power users, the "intentionally not in
GUI" list is the next set of features to add; the wire types already
exist in `Sources/SpotuifyKit/Models/DaemonRequest.swift` and
`DaemonEvent.swift`.

---

## 16. File index

| File | Lines | Role |
|---|---|---|
| `Sources/Spotuify/SpotuifyApp.swift` | 214 | App entry, 4 scenes, all keyboard menus |
| `Sources/Spotuify/AppDelegate.swift` | 13 | font registration, keep alive after last window close |
| `Sources/SpotuifyKit/SpotuifyKit.swift` | — | framework umbrella |
| `Sources/SpotuifyKit/Models/DaemonRequest.swift` | 524 | full request roster + `PlaybackCommand` + `SearchSource` / `SearchScope` / `RepeatMode` / `AnalyticsTopKind` / `AnalyticsHabitWindow` / `AnalyticsSearchMode` / `AnalyticsSinceWindow` / `VizSourceKind` / `SyncTarget` / `OperationSource` |
| `Sources/SpotuifyKit/Models/DaemonEvent.swift` | 136 | 25 daemon events with `Decodable` synthesis + `.unknown` fallback |
| `Sources/SpotuifyKit/Models/Response.swift` | 175 | `ResponsePayload`, `ResponseData` (20 kinds), `DaemonError`, `CommandReceipt`, `DaemonStatus`, `UpgradeHint`, `UpdateStatus` |
| `Sources/SpotuifyKit/Models/Domain.swift` | — | `MediaKind`, `MediaItem`, `ArtistRef`, `ListenSession`, `Device`, `Playback`, `Queue`, `Playlist`, `LyricLine`, `SyncedLyrics`, `ClientSeed` |
| `Sources/SpotuifyKit/Models/Reminders.swift` | — | `Recurrence`, `ReminderState`, `NotificationState`, `Reminder`, `ReminderNotification` |
| `Sources/SpotuifyKit/Models/Wire.swift` | — | `AnyKey`, `IpcMessage`, `InboundPayload`, `OutboundMessage`, encode/decode helpers |
| `Sources/SpotuifyKit/Networking/DaemonConnection.swift` | 205 | actor: `connect`, `request`, `subscribeEvents`, `events` (AsyncStream), `waitUntilClosed` |
| `Sources/SpotuifyKit/Networking/IPCSocket.swift` | 103 | AF_UNIX read/write loop, `DispatchSourceRead`, frame callback |
| `Sources/SpotuifyKit/Networking/FrameCodec.swift` | 57 | `FrameDecoder` (4-byte big-endian length, 16 MB cap) + `FrameEncoder` |
| `Sources/SpotuifyKit/Networking/SocketPath.swift` | 27 | env-driven path resolution |
| `Sources/SpotuifyKit/Networking/ConnectionState.swift` | 34 | `ConnectionState`, `DaemonReadiness`, `DaemonConnectionError` |
| `Sources/SpotuifyKit/Networking/DaemonLauncher.swift` | — | binary resolution + `ensureRunning` + bundled CLI install |
| `Sources/SpotuifyKit/Networking/DaemonControl.swift` | — | brew install/upgrade, `startDaemon`, `homebrewAvailable` |
| `Sources/SpotuifyKit/Networking/CLIRunner.swift` | — | `spotuify <args>` wrapper with bounded timeout |
| `Sources/SpotuifyKit/Stores/AppModel.swift` | 511 | top-level coordinator + event router + supervisor + command helpers |
| `Sources/SpotuifyKit/Stores/PlayerStore.swift` | 87 | playback/queue/devices + 250 ms interpolated ticker |
| `Sources/SpotuifyKit/Stores/SearchStore.swift` | — | 350 ms debounce, results, grouped by kind |
| `Sources/SpotuifyKit/Stores/LibraryStore.swift` | — | playlists / liked / albums / shows / followed / history / playlistTracks + event-driven reload |
| `Sources/SpotuifyKit/Stores/LyricsStore.swift` | — | lyrics + activeIndex(positionMs:) |
| `Sources/SpotuifyKit/Stores/RemindersStore.swift` | — | reminders + notifications + open/unseen counts |
| `Sources/SpotuifyKit/Stores/PodcastsStore.swift` | — | mode, source, episode sort, episode feed, show search |
| `Sources/SpotuifyKit/Stores/ConfigStore.swift` | — | 450 ms debounced `spotuify config set` + reload |
| `Sources/SpotuifyKit/Stores/VizStore.swift` | — | 12-band spectrum buffer |
| `Sources/SpotuifyKit/Services/AppUpdater.swift` | — | DMG-based self-update (download, sha256, mount, swap) |
| `Sources/Spotuify/Views/Shell/AppShell.swift` | 257 | root layout, sidebar/detail split, banners, toasts, global side panel, due-inbox sheet |
| `Sources/Spotuify/Views/Shell/Sidebar.swift` | 94 | native `List`, connection badge, theme-aware background |
| `Sources/Spotuify/Views/Shell/DaemonGateView.swift` | 251 | install / start / update / docs; copyable CLI rows |
| `Sources/Spotuify/Views/Shell/Destination.swift` | 64 | `Destination` enum (10 cases) + `Navigator` (⌘1…⌘0 order) |
| `Sources/Spotuify/Views/Player/NowPlayingView.swift` | 588 | immersive stage (artwork / visualizer / lyrics / up next) + transport + seek + like + nav links |
| `Sources/Spotuify/Views/Player/NowPlayingBar.swift` | — | always-visible footer with global rail toggles |
| `Sources/Spotuify/Views/Player/DeviceMenu.swift` | — | compact device picker |
| `Sources/Spotuify/Views/Player/SeekBar.swift` | — | controlled component (progress, onSeek) |
| `Sources/Spotuify/Views/Player/VolumeControl.swift` | — | 4-stage glyph + drag |
| `Sources/Spotuify/Views/Search/SearchView.swift` | — | TextField, source picker, filter chips, sort menu, grouped results |
| `Sources/Spotuify/Views/Library/LibraryView.swift` | — | `LikedSongsView`, `AlbumsView`, `ArtistsView`, `CollectionHeader`, `ArtworkTile` |
| `Sources/Spotuify/Views/Playlists/PlaylistsView.swift` | — | `PlaylistsView`, `PlaylistDetailView` |
| `Sources/Spotuify/Views/Podcasts/PodcastsView.swift` | — | mode/source/sort pickers, shows grid, episodes list |
| `Sources/Spotuify/Views/History/HistoryView.swift` | — | recent / sessions, `SessionDetailView` |
| `Sources/Spotuify/Views/Reminders/RemindersView.swift` | — | Inbox + Scheduled sections, `NotificationRow`, `ReminderRow`, `DueRemindersSheet` |
| `Sources/Spotuify/Views/Reminders/ReminderPickerView.swift` | — | preset grid + `DatePicker` + `Recurrence` + `TextField` + `model.createReminder` |
| `Sources/Spotuify/Views/Lyrics/LyricsView.swift` | — | auto-scrolling synced lyrics, tap-to-seek |
| `Sources/Spotuify/Views/Devices/DevicesView.swift` | — | device rows |
| `Sources/Spotuify/Views/Settings/SettingsView.swift` | 407 | 9 panes; `AppearancePaneBody`, `ThemeTile`, `UpdatesPaneBody`, `SecretField` |
| `Sources/Spotuify/Views/MiniPlayer/MiniPlayerView.swift` | 173 | 3 sizes, floating window, palette wash |
| `Sources/Spotuify/Views/MenuBar/MenuBarView.swift` | 103 | 320 pt popover, palette header, transport, footer |
| `Sources/Spotuify/Views/Detail/MediaDetailViews.swift` | — | `DetailHeader` + 5 detail types + `mediaDetailDestinations()` |
| `Sources/Spotuify/Views/Common/AsyncCoverImage.swift` | — | `CoverArtCache` consumer, square / circle clip |
| `Sources/Spotuify/Views/Common/MediaRow.swift` | — | universal track/episode list row with right-click menu |
| `Sources/Spotuify/Views/Common/MediaItemMenu.swift` | — | per-kind `Menu` (play / queue / like / follow / remind) |
| `Sources/Spotuify/Views/Common/TrackListView.swift` | — | generic filterable / sortable list with `LayoutToggle`, `TrackCard` |
| `Sources/Spotuify/Views/Common/CollectionView.swift` | — | grid/list with `ArtworkTile` + per-surface `storageKey` |
| `Sources/Spotuify/Views/Common/VisualizerView.swift` | 191 | bars / circular / wave, palette-tinted |
| `Sources/Spotuify/Views/Common/SkeletonPlaceholder.swift` | — | `SkeletonRows` + `SkeletonTiles` |
| `Sources/Spotuify/System/KeyboardController.swift` | — | bare Space → toggle play/pause |
| `Sources/Spotuify/System/SystemMediaController.swift` | — | `MPRemoteCommandCenter` + `MPNowPlayingInfoCenter` |
| `Sources/Spotuify/System/ReminderNotificationScheduler.swift` | — | `UNUserNotificationCenter` delegate + category `REMINDER` |
| `Sources/Spotuify/System/AppRelaunch.swift` | — | poll old PID, `exec /usr/bin/open`, terminate |
| `Sources/Spotuify/System/CoverArtCache.swift` | — | NSCache + daemon `.coverArt` + URLSession fallback |
| `Sources/Spotuify/System/TerminalLauncher.swift` | — | open Terminal.app running the given commands |
| `Sources/Spotuify/Theme/*.swift` | 12 | tokens, palette, preference, font, themed wrapper |
| `Tests/SpotuifyKitTests/*.swift` | — | `FrameCodecTests`, `RequestEncodingTests`, `WireDecodingTests`, `ProtocolParityTests` (cmd-string diff vs fixture), `PlayerStoreTests`, `LiveDaemonTests`, `SmokeTests` |

---

## 17. One-paragraph elevator pitch

The Spotuify macOS client is a 4-scene SwiftUI app (Player window, Mini
Player floating panel, Settings window, Menubar popover) that is a pure
view of a local Rust daemon. The app talks to the daemon over a length-
delimited JSON Unix socket using 78 typed request cases and 25 typed
events; the daemon owns playback, queue, devices, library, search, lyrics,
reminders, and updates; the app renders and dispatches. Every affordance
in the app maps 1-to-1 to a `spotuify <subcommand>` so the CLI is the
authoritative spec for what the app does. A porting client needs three
things: the wire format (§3), the event router (§4), and the 10
destinations + 9 settings panes (§§5–6). The rest is theme chrome and
macOS-specific OS integration (§10) that maps cleanly to each platform's
equivalents.
