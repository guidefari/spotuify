import Observation
import SwiftUI

/// Shared current-destination state, so both the sidebar (`AppShell`) and the
/// app-level keyboard `Commands` (⌘1–0) can drive view navigation.
@MainActor
@Observable
final class Navigator {
    var selection: Destination = .nowPlaying

    /// Numeric-key order for ⌘1…⌘9, ⌘0 (mirrors the TUI's 1–9/0 + sidebar).
    static let numbered: [Destination] = [
        .nowPlaying, .search, .likedSongs, .albums,
        .artists, .podcasts, .playlists, .history, .devices,
    ]
}

/// Sidebar destinations. The queue is no longer a top-level destination;
/// it lives behind the Now Playing mode switch (and as a global side panel
/// from the footer) so the sidebar stays focused on library + nav.
enum Destination: String, CaseIterable, Identifiable {
    case nowPlaying
    case search
    case likedSongs
    case albums
    case artists
    case podcasts
    case playlists
    case history
    case notifications
    case devices

    var id: String { rawValue }

    var title: String {
        switch self {
        case .nowPlaying: "Now Playing"
        case .search: "Search"
        case .likedSongs: "Liked Songs"
        case .albums: "Albums"
        case .artists: "Artists"
        case .podcasts: "Podcasts"
        case .playlists: "Playlists"
        case .history: "History"
        case .notifications: "Notifications"
        case .devices: "Devices"
        }
    }

    var icon: String {
        switch self {
        case .nowPlaying: "play.circle.fill"
        case .search: "magnifyingglass"
        case .likedSongs: "heart.fill"
        case .albums: "square.stack.fill"
        case .artists: "music.mic"
        case .podcasts: "mic.fill"
        case .playlists: "music.note.list"
        case .history: "clock.arrow.circlepath"
        case .notifications: "bell.fill"
        case .devices: "hifispeaker.2.fill"
        }
    }
}
