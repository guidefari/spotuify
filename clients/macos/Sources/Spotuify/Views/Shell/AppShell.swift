import SwiftUI
import SpotuifyKit

/// Root layout: sidebar + destination content, with the always-visible
/// NowPlayingBar pinned to the bottom across the full width.
struct AppShell: View {
    @Environment(AppModel.self) private var model
    @Environment(ArtworkTheme.self) private var theme
    @State private var selection: Destination = .nowPlaying
    /// Shared with NowPlayingView: when it minimises its controls for full art,
    /// the footer transport reappears so playback stays controllable.
    @AppStorage("nowPlayingMinimized") private var nowPlayingMinimized = false
    /// The global right-hand panel (queue / lyrics), toggled from the footer bar
    /// and available on every page (Now Playing has its own panels instead).
    @AppStorage("globalSidePanel") private var globalPanelRaw = GlobalPanel.none.rawValue
    private var globalPanel: GlobalPanel { GlobalPanel(rawValue: globalPanelRaw) ?? .none }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                NavigationSplitView {
                    Sidebar(selection: $selection)
                        .navigationSplitViewColumnWidth(min: 200, ideal: Theme.sidebarWidth, max: 260)
                } detail: {
                    destinationView
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .navigationSplitViewStyle(.balanced)
                // Global queue/lyrics rail — shown on every page except Now
                // Playing (which has its own in-stage panels).
                if globalPanel != .none && selection != .nowPlaying {
                    Divider()
                    GlobalSidePanel(panel: globalPanel) { globalPanelRaw = GlobalPanel.none.rawValue }
                        .frame(width: 340)
                        .transition(.move(edge: .trailing).combined(with: .opacity))
                }
            }
            // The immersive Now Playing page has its own full transport, so hide
            // the bottom bar there — unless its controls are minimised for full
            // art, in which case the footer is where the transport lives.
            if selection != .nowPlaying || nowPlayingMinimized {
                Divider()
                NowPlayingBar()
            }
        }
        .animation(.easeInOut(duration: 0.25), value: globalPanel)
        .frame(minWidth: 880, minHeight: 620)
        .overlay(alignment: .top) { bannerView }
        .tint(theme.accent)
        .environment(theme)
        .task(id: model.player.currentItem?.imageURL) {
            await theme.update(for: model.player.currentItem?.imageURL)
        }
        .sheet(
            isPresented: Binding(
                get: { model.presentDueInbox },
                set: { model.presentDueInbox = $0 })
        ) {
            DueRemindersSheet { selection = .notifications }
        }
    }

    @ViewBuilder
    private var destinationView: some View {
        switch selection {
        case .nowPlaying: NowPlayingView()
        case .search: SearchView()
        case .likedSongs: LikedSongsView()
        case .albums: AlbumsView()
        case .artists: ArtistsView()
        case .podcasts: PodcastsView()
        case .playlists: PlaylistsView()
        case .queue: QueueView()
        case .history: HistoryView()
        case .notifications: RemindersView()
        case .devices: DevicesView()
        }
    }

    @ViewBuilder
    private var bannerView: some View {
        if let banner = model.banner {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                Text(banner).font(.callout)
                Spacer()
                Button {
                    model.clearBanner()
                } label: { Image(systemName: "xmark") }
                    .buttonStyle(.plain)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(.thinMaterial, in: Capsule())
            .foregroundStyle(.primary)
            .padding(.top, 10)
            .shadow(radius: 6, y: 2)
            .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}

/// The global right-hand rail: up-next queue or synced lyrics, openable from
/// any page via the footer bar (Apple-Music-style).
enum GlobalPanel: String { case none, queue, lyrics }

struct GlobalSidePanel: View {
    @Environment(ArtworkTheme.self) private var theme
    let panel: GlobalPanel
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(panel == .queue ? "Up Next" : "Lyrics")
                    .font(.headline)
                Spacer()
                Button(action: onClose) {
                    Image(systemName: "xmark").font(.system(size: 12, weight: .bold))
                }
                .buttonStyle(.plain).foregroundStyle(.secondary)
                .help("Close")
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            Divider()
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(.horizontal, 10)
        }
        .background(.regularMaterial)
    }

    @ViewBuilder
    private var content: some View {
        switch panel {
        case .queue: NowPlayingQueue(accent: theme.accent)
        case .lyrics: LyricsView()
        case .none: EmptyView()
        }
    }
}

/// Placeholder for destinations filled in by later phases.
struct ComingSoonView: View {
    let destination: Destination

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: destination.icon)
                .font(.system(size: 44))
                .foregroundStyle(.tertiary)
            Text(destination.title)
                .font(.title2.bold())
            Text("Coming soon")
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background)
    }
}
