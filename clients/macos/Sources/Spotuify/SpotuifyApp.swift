import SwiftUI
import SpotuifyKit

@main
struct SpotuifyApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var model = AppModel()
    @State private var theme = ArtworkTheme()
    @State private var navigator = Navigator()

    var body: some Scene {
        // `Window` (not `WindowGroup`) so there is exactly one player window —
        // re-invoking `openWindow(id: "player")` focuses the existing one
        // instead of spawning a new window each time.
        Window("Spotuify", id: "player") {
            ThemedView(usesArtworkAccent: true) {
                RootView()
            }
            .environment(model)
            .environment(theme)
            .environment(navigator)
            .task {
                // Self-contained install: drop the bundled daemon+CLI onto
                // the user's PATH so the backend is available everywhere.
                DaemonLauncher.installBundledCLIIfNeeded()
                model.start()
                SystemMediaController.shared.configure(model: model)
                KeyboardController.shared.configure(model: model)
                ReminderNotificationScheduler.shared.configure(model: model)
            }
            .onChange(of: model.player.playback) { _, _ in
                Task { await SystemMediaController.shared.updateNowPlaying(player: model.player) }
            }
            // The displayed track can also change via a queue update (which
            // doesn't touch `playback`), and play/pause must refresh the
            // Now Playing state — republish on both.
            .onChange(of: model.player.currentItem?.uri) { _, _ in
                Task { await SystemMediaController.shared.updateNowPlaying(player: model.player) }
            }
            .onChange(of: model.player.isPlaying) { _, _ in
                Task { await SystemMediaController.shared.updateNowPlaying(player: model.player) }
            }
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 980, height: 720)
        .commands {
            // We swapped the `Settings` scene for a plain `Window` (so the
            // titlebar matches the main app), which means the auto-injected
            // "Settings…" menu item is gone. Re-add it at its standard slot
            // and keep the "Check for Updates…" item right after.
            CommandGroup(replacing: .appSettings) {
                SettingsCommand()
                CheckForUpdatesCommand(model: model)
            }
            CommandGroup(after: .windowArrangement) {
                MiniPlayerCommand()
            }
            CommandMenu("Playback") { PlaybackCommands(model: model) }
            CommandMenu("Go") { GoCommands(navigator: navigator) }
        }

        // Single floating HUD window — likewise reused, never duplicated.
        // The Mini Player pins `theme.accent` itself on its content (the album
        // stage), but the system chrome around the floating window still needs
        // to follow the user's chosen color scheme. `ThemedView`'s inner
        // `.tint(theme.accent)` is harmless here because the content overrides
        // it; the outer `.preferredColorScheme` is what actually changes.
        Window("Mini Player", id: "mini-player") {
            ThemedView(usesArtworkAccent: true) {
                MiniPlayerView()
            }
            .environment(model)
            .environment(theme)
            .task { model.start() }
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 320, height: 380)

        MenuBarExtra("Spotuify", systemImage: "music.note") {
            ThemedView(usesArtworkAccent: true) {
                MenuBarView()
            }
            .environment(model)
            .environment(theme)
        }
        .menuBarExtraStyle(.window)

        // Plain `Window` (not `Settings`) so the titlebar chrome matches the
        // player window: standard traffic lights + macOS sidebar toggle. The
        // `Settings` scene on macOS 26 ships a Liquid Glass "go back" pill
        // that doesn't match the main app's look. We re-add the ⌘, shortcut
        // and "Settings…" menu item below via `appSettings` CommandGroup.
        Window("Settings", id: "settings") {
            ThemedView(usesArtworkAccent: false) {
                SettingsView()
            }
            .environment(model)
            .environment(theme)
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 760, height: 540)
    }
}

/// Gates the player UI behind a daemon presence + version check.
struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Group {
            switch model.readiness {
            case .ready:
                AppShell()
            default:
                DaemonGateView(readiness: model.readiness)
            }
        }
        // Viz focus is a per-client vote on the daemon; report ours so
        // an unfocused TUI can't throttle this app's visualizer (and
        // vice versa). Also vote on connect, since a stale vote from a
        // previous run may still be on file.
        .onReceive(NotificationCenter.default.publisher(
            for: NSApplication.didBecomeActiveNotification
        )) { _ in
            model.setVizFocus(true)
        }
        .onReceive(NotificationCenter.default.publisher(
            for: NSApplication.willResignActiveNotification
        )) { _ in
            model.setVizFocus(false)
        }
        .onChange(of: model.isReady) { _, ready in
            if ready { model.setVizFocus(NSApp.isActive) }
        }
        // Window closed (app keeps running via the menu bar): nothing
        // shows the visualizer anymore, so withdraw our focused vote —
        // a stale `true` pinned the daemon's spectrum broadcast at
        // full rate into a windowless app indefinitely.
        .onDisappear {
            model.setVizFocus(false)
        }
    }
}

/// Global playback keyboard control (Space / ⌘arrows / ⌘⇧S / ⌘⇧R), shown in
/// the Playback menu so the shortcuts are discoverable. Space play/pause yields
/// to a focused text field (it inserts a space there instead).
private struct PlaybackCommands: View {
    let model: AppModel
    var body: some View {
        Button("Play / Pause") { model.togglePlayPause() }
            .keyboardShortcut(.space, modifiers: [])
        Button("Next") { model.next() }
            .keyboardShortcut(.rightArrow, modifiers: .command)
        Button("Previous") { model.previous() }
            .keyboardShortcut(.leftArrow, modifiers: .command)
        Divider()
        Button("Volume Up") { model.setVolume(Int(model.player.volumePercent ?? 0) + 5) }
            .keyboardShortcut(.upArrow, modifiers: .command)
        Button("Volume Down") { model.setVolume(Int(model.player.volumePercent ?? 0) - 5) }
            .keyboardShortcut(.downArrow, modifiers: .command)
        Divider()
        Button("Toggle Shuffle") { model.toggleShuffle() }
            .keyboardShortcut("s", modifiers: [.command, .shift])
        Button("Cycle Repeat") { model.cycleRepeat() }
            .keyboardShortcut("r", modifiers: [.command, .shift])
    }
}

/// View navigation: ⌘1…⌘9, ⌘0 jump to each destination (mirrors the TUI's
/// 1–9/0 and the sidebar order).
private struct GoCommands: View {
    let navigator: Navigator
    var body: some View {
        ForEach(Array(Navigator.numbered.enumerated()), id: \.element.id) { index, dest in
            Button(dest.title) { navigator.selection = dest }
                .keyboardShortcut(
                    KeyEquivalent(Character("\((index + 1) % 10)")), modifiers: .command)
        }
    }
}

/// "Check for Updates…" in the app menu — forces a fresh check and opens
/// Settings so the result (Updates pane + banner) is visible.
private struct CheckForUpdatesCommand: View {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Button("Check for Updates…") {
            model.checkUpdate(force: true)
            openWindow(id: "settings")
        }
    }
}

/// "Settings…" menu item with the ⌘, shortcut. Provided manually because we
/// use a `Window` scene instead of the `Settings` scene (which would normally
/// inject this command for free).
private struct SettingsCommand: View {
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Button("Settings…") { openWindow(id: "settings") }
            .keyboardShortcut(",", modifiers: .command)
    }
}

/// Menu command + ⌘⇧M shortcut to open the floating mini-player.
private struct MiniPlayerCommand: View {
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Button("Mini Player") { openWindow(id: "mini-player") }
            .keyboardShortcut("m", modifiers: [.command, .shift])
    }
}
