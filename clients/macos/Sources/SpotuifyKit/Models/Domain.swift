import Foundation

// Core domain types mirroring `spotuify_core`. Field names map the daemon's
// snake_case JSON via explicit CodingKeys (the daemon uses no rename_all on
// these structs, so keys are verbatim). All types are immutable value types
// and Sendable so they can cross from the IO actor to @MainActor stores.

public enum MediaKind: String, Codable, Sendable, Hashable {
    case track, episode, show, album, artist, playlist
    case other

    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = MediaKind(rawValue: raw) ?? .other
    }
}

/// A named reference to an artist carrying its URI, so a track/album row can
/// navigate straight to the artist. Mirrors `spotuify_core::ArtistRef`.
public struct ArtistRef: Codable, Sendable, Hashable, Identifiable {
    public let name: String
    public let uri: String

    public init(name: String, uri: String) {
        self.name = name
        self.uri = uri
    }

    public var id: String { uri }
}

public struct MediaItem: Codable, Sendable, Hashable, Identifiable {
    public let spotifyID: String?
    public let uri: String
    public let name: String
    public let subtitle: String
    public let context: String
    public let durationMs: UInt64
    /// Default (medium) image URL — the size Spotify ships closest to 300px.
    /// Right for 200–300pt list / grid tiles, menu-bar covers, and
    /// system-media art. Use `imageURLSmall` for thumbnails and
    /// `imageURLLarge` for the now-playing hero.
    public let imageURL: String?
    /// Smallest image URL Spotify returned (≈ 64px source). Right for
    /// 40–50pt row thumbnails (now-playing footer, queue rows, history
    /// chips, reminder rows). Falls back to `imageURL` when Spotify only
    /// returned a single size.
    public let imageURLSmall: String?
    /// Largest image URL Spotify returned (≈ 640px+, sometimes 1200+ for
    /// shows / podcasts). Right for the now-playing hero (contained square
    /// or full-bleed). Falls back to `imageURL` when Spotify only returned
    /// a single size.
    public let imageURLLarge: String?
    public let kind: MediaKind
    public let source: String?
    public let freshness: String?
    public let explicit: Bool?
    public let isPlayable: Bool?
    public let album: String?
    public let addedAtMs: Int64?
    public let resumePositionMs: UInt64?
    public let fullyPlayed: Bool?
    public let releaseDate: String?
    /// Spotify's per-artist `album_group` (album/single/compilation/appears_on);
    /// drives the discography sections. `nil` for non-album items.
    public let albumGroup: String?
    /// Whether this album is already in the user's library (tagged by the
    /// daemon for an artist's discography). `nil` when not applicable.
    public let inLibrary: Bool?
    /// Album URI for a track, enabling navigation to the album. `nil` when
    /// unknown (older cached rows / non-track items).
    public let albumURI: String?
    /// Contributing artists with URIs, enabling navigation to each artist.
    /// Empty when unknown.
    public let artists: [ArtistRef]
    /// Primary genre, when known (Spotify carries it on the artist/album, so
    /// it's populated best-effort). `nil` when unknown.
    public let genre: String?

    public init(
        spotifyID: String? = nil,
        uri: String,
        name: String,
        subtitle: String = "",
        context: String = "",
        durationMs: UInt64 = 0,
        imageURL: String? = nil,
        imageURLSmall: String? = nil,
        imageURLLarge: String? = nil,
        kind: MediaKind = .track,
        source: String? = nil,
        freshness: String? = nil,
        explicit: Bool? = nil,
        isPlayable: Bool? = nil,
        album: String? = nil,
        addedAtMs: Int64? = nil,
        resumePositionMs: UInt64? = nil,
        fullyPlayed: Bool? = nil,
        releaseDate: String? = nil,
        albumGroup: String? = nil,
        inLibrary: Bool? = nil,
        albumURI: String? = nil,
        artists: [ArtistRef] = [],
        genre: String? = nil
    ) {
        self.spotifyID = spotifyID
        self.uri = uri
        self.name = name
        self.subtitle = subtitle
        self.context = context
        self.durationMs = durationMs
        self.imageURL = imageURL
        self.imageURLSmall = imageURLSmall
        self.imageURLLarge = imageURLLarge
        self.kind = kind
        self.source = source
        self.freshness = freshness
        self.explicit = explicit
        self.isPlayable = isPlayable
        self.album = album
        self.addedAtMs = addedAtMs
        self.resumePositionMs = resumePositionMs
        self.fullyPlayed = fullyPlayed
        self.releaseDate = releaseDate
        self.albumGroup = albumGroup
        self.inLibrary = inLibrary
        self.albumURI = albumURI
        self.artists = artists
        self.genre = genre
    }

    /// Stable identity for SwiftUI. The Spotify `id` is optional and not
    /// always unique across kinds, but `uri` is the canonical handle.
    public var id: String { uri }

    /// Best album label for display: the dedicated field, falling back to
    /// `context` (which the daemon fills with the album for tracks).
    public var albumLabel: String? {
        if let album, !album.isEmpty { return album }
        return context.isEmpty ? nil : context
    }

    /// Episode listened state.
    public var isFullyPlayed: Bool { fullyPlayed == true }
    public var isInProgress: Bool { (resumePositionMs ?? 0) > 0 && !isFullyPlayed }

    /// A secondary metadata line for collection rows/tiles (distinct from the
    /// artist/owner `subtitle`): year + track count for albums, follower count
    /// for artists, episode count for shows, track count for playlists. `nil`
    /// when there's nothing extra to show.
    public var metaLine: String? {
        var parts: [String] = []
        if kind == .album, let releaseDate, releaseDate.count >= 4 {
            parts.append(String(releaseDate.prefix(4)))
        }
        if !context.isEmpty { parts.append(context) }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// Synthetic artist items (kind `.artist`) for click-through navigation
    /// from a track/album row. Only artists carrying a URI are navigable.
    public var artistNavItems: [MediaItem] {
        artists.filter { !$0.uri.isEmpty }.map {
            MediaItem(uri: $0.uri, name: $0.name, kind: .artist)
        }
    }

    /// Synthetic album item (kind `.album`) for navigating from a track to its
    /// album. `nil` when the album URI is unknown.
    public var albumNavItem: MediaItem? {
        guard let albumURI, !albumURI.isEmpty else { return nil }
        return MediaItem(
            uri: albumURI, name: albumLabel ?? "Album", imageURL: imageURL, kind: .album)
    }

    enum CodingKeys: String, CodingKey {
        case spotifyID = "id"
        case uri, name, subtitle, context
        case durationMs = "duration_ms"
        case imageURL = "image_url"
        case imageURLSmall = "image_url_small"
        case imageURLLarge = "image_url_large"
        case kind, source, freshness, explicit
        case isPlayable = "is_playable"
        case album
        case addedAtMs = "added_at_ms"
        case resumePositionMs = "resume_position_ms"
        case fullyPlayed = "fully_played"
        case releaseDate = "release_date"
        case albumGroup = "album_group"
        case inLibrary = "in_library"
        case albumURI = "album_uri"
        case artists
        case genre
    }

    // Custom decoder so the daemon's `skip_serializing_if`'d fields (notably
    // `artists`, omitted when empty) decode to sensible defaults instead of
    // failing. Encoding stays synthesized.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        spotifyID = try c.decodeIfPresent(String.self, forKey: .spotifyID)
        uri = try c.decode(String.self, forKey: .uri)
        name = try c.decode(String.self, forKey: .name)
        subtitle = try c.decodeIfPresent(String.self, forKey: .subtitle) ?? ""
        context = try c.decodeIfPresent(String.self, forKey: .context) ?? ""
        durationMs = try c.decodeIfPresent(UInt64.self, forKey: .durationMs) ?? 0
        imageURL = try c.decodeIfPresent(String.self, forKey: .imageURL)
        imageURLSmall = try c.decodeIfPresent(String.self, forKey: .imageURLSmall)
        imageURLLarge = try c.decodeIfPresent(String.self, forKey: .imageURLLarge)
        kind = try c.decodeIfPresent(MediaKind.self, forKey: .kind) ?? .track
        source = try c.decodeIfPresent(String.self, forKey: .source)
        freshness = try c.decodeIfPresent(String.self, forKey: .freshness)
        explicit = try c.decodeIfPresent(Bool.self, forKey: .explicit)
        isPlayable = try c.decodeIfPresent(Bool.self, forKey: .isPlayable)
        album = try c.decodeIfPresent(String.self, forKey: .album)
        addedAtMs = try c.decodeIfPresent(Int64.self, forKey: .addedAtMs)
        resumePositionMs = try c.decodeIfPresent(UInt64.self, forKey: .resumePositionMs)
        fullyPlayed = try c.decodeIfPresent(Bool.self, forKey: .fullyPlayed)
        releaseDate = try c.decodeIfPresent(String.self, forKey: .releaseDate)
        albumGroup = try c.decodeIfPresent(String.self, forKey: .albumGroup)
        inLibrary = try c.decodeIfPresent(Bool.self, forKey: .inLibrary)
        albumURI = try c.decodeIfPresent(String.self, forKey: .albumURI)
        artists = try c.decodeIfPresent([ArtistRef].self, forKey: .artists) ?? []
        genre = try c.decodeIfPresent(String.self, forKey: .genre)
    }
}

/// One listening session — a run of consecutively-played tracks. Mirrors
/// `spotuify_protocol::ListenSession`.
public struct ListenSession: Codable, Sendable, Hashable, Identifiable {
    public let sessionID: String
    public let startedAtMs: Int64
    public let endedAtMs: Int64
    public let trackCount: UInt32
    public let contextLabel: String?
    public let tracks: [MediaItem]

    public var id: String { sessionID }

    enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case startedAtMs = "started_at_ms"
        case endedAtMs = "ended_at_ms"
        case trackCount = "track_count"
        case contextLabel = "context_label"
        case tracks
    }
}

public struct Device: Codable, Sendable, Hashable, Identifiable {
    public let deviceID: String?
    public let name: String
    public let kind: String
    public let isActive: Bool
    public let isRestricted: Bool
    public let volumePercent: UInt8?
    public let supportsVolume: Bool

    public var id: String { deviceID ?? name }

    enum CodingKeys: String, CodingKey {
        case deviceID = "id"
        case name
        case kind = "type"
        case isActive = "is_active"
        case isRestricted = "is_restricted"
        case volumePercent = "volume_percent"
        case supportsVolume = "supports_volume"
    }
}

public struct Playback: Codable, Sendable, Equatable {
    public let item: MediaItem?
    public let device: Device?
    public let isPlaying: Bool
    public let progressMs: UInt64
    public let shuffle: Bool
    public let repeatMode: String
    public let sampledAtMs: Int64?
    public let providerTimestampMs: Int64?
    public let source: String?

    enum CodingKeys: String, CodingKey {
        case item, device
        case isPlaying = "is_playing"
        case progressMs = "progress_ms"
        case shuffle
        case repeatMode = "repeat"
        case sampledAtMs = "sampled_at_ms"
        case providerTimestampMs = "provider_timestamp_ms"
        case source
    }
}

public struct Queue: Codable, Sendable, Equatable {
    public let currentlyPlaying: MediaItem?
    public let items: [MediaItem]
    /// `session_active` / `as_of_ms` carry `#[serde(default)]` on the daemon,
    /// so they may be absent on older snapshots — modelled as optionals.
    public let sessionActive: Bool?
    public let asOfMs: Int64?

    public var isSessionActive: Bool { sessionActive ?? false }

    enum CodingKeys: String, CodingKey {
        case currentlyPlaying = "currently_playing"
        case items
        case sessionActive = "session_active"
        case asOfMs = "as_of_ms"
    }
}

public struct Playlist: Codable, Sendable, Hashable, Identifiable {
    public let id: String
    public let name: String
    public let owner: String
    public let tracksTotal: UInt64
    public let imageURL: String?
    public let snapshotID: String?

    enum CodingKeys: String, CodingKey {
        case id, name, owner
        case tracksTotal = "tracks_total"
        case imageURL = "image_url"
        case snapshotID = "snapshot_id"
    }
}

public struct LyricLine: Codable, Sendable, Hashable {
    public let startMs: UInt64
    public let text: String
    public let isRtl: Bool

    enum CodingKeys: String, CodingKey {
        case startMs = "start_ms"
        case text
        case isRtl = "is_rtl"
    }
}

public struct SyncedLyrics: Codable, Sendable, Equatable {
    public let provider: String
    public let trackURI: String
    public let lines: [LyricLine]
    public let fetchedAtMs: Int64
    public let synced: Bool
    public let language: String?
    public let sourceURL: String?

    enum CodingKeys: String, CodingKey {
        case provider
        case trackURI = "track_uri"
        case lines
        case fetchedAtMs = "fetched_at_ms"
        case synced, language
        case sourceURL = "source_url"
    }

    /// Index of the line active at `positionMs` (with a per-track `offsetMs`
    /// tweak), mirroring `spotuify_core::active_lyric_line_index`.
    public func activeLineIndex(positionMs: UInt64, offsetMs: Int64) -> Int? {
        guard !lines.isEmpty else { return nil }
        let adjusted: UInt64
        if offsetMs < 0 {
            adjusted = positionMs >= UInt64(-offsetMs) ? positionMs - UInt64(-offsetMs) : 0
        } else {
            adjusted = positionMs &+ UInt64(offsetMs)
        }
        let count = lines.prefix { $0.startMs <= adjusted }.count
        return count == 0 ? nil : count - 1
    }
}

/// Read-only startup snapshot returned by `client-seed`.
public struct ClientSeed: Decodable, Sendable {
    public let playback: Playback
    public let queue: Queue
    public let devices: [Device]
    public let recent: [MediaItem]
}
