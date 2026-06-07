import SwiftUI

/// Sidebar destinations.
enum Destination: String, CaseIterable, Identifiable {
    case nowPlaying
    case queue
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
        case .queue: "Queue"
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
        case .queue: "list.bullet"
        case .history: "clock.arrow.circlepath"
        case .notifications: "bell.fill"
        case .devices: "hifispeaker.2.fill"
        }
    }
}
