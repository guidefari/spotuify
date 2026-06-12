import SwiftUI
import SpotuifyKit

struct PlaylistsView: View {
    @Environment(AppModel.self) private var model

    /// Sidebar `Playlist`s as `MediaItem`s so they flow through the shared
    /// grid/list `CollectionView` and open via `mediaDetailDestinations`.
    private var items: [MediaItem] {
        model.library.playlists.map { playlist in
            MediaItem(
                uri: "spotify:playlist:\(playlist.id)",
                name: playlist.name,
                subtitle: playlist.owner,
                context: "\(playlist.tracksTotal) tracks",
                imageURL: playlist.imageURL,
                kind: .playlist)
        }
    }

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 0) {
                EditorialPageHeader("Playlists")
                Divider()
                if model.library.loadingPlaylists && model.library.playlists.isEmpty {
                    SkeletonTiles()
                } else if model.library.playlists.isEmpty {
                    ContentUnavailableView("No playlists", systemImage: "music.note.list")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    CollectionView(items: items, storageKey: "playlistsLayout")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .mediaDetailDestinations()
        }
        .background(.background)
        .task { await model.library.loadPlaylists() }
    }
}

struct PlaylistDetailView: View {
    @Environment(AppModel.self) private var model
    let playlist: Playlist

    private var tracks: [MediaItem] { model.library.tracks(for: playlist) }

    private var playlistURI: String { "spotify:playlist:\(playlist.id)" }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 16) {
                AsyncCoverImage(url: playlist.imageURL, cornerRadius: 10)
                    .frame(width: 120, height: 120)
                    .shadow(radius: 8, y: 4)
                VStack(alignment: .leading, spacing: 8) {
                    Text(playlist.name).font(.displayHero(32)).lineLimit(2).minimumScaleFactor(0.6)
                    Text("\(playlist.tracksTotal) tracks · \(playlist.owner)")
                        .foregroundStyle(.secondary)
                    HStack(spacing: 10) {
                        Button { model.play(uri: playlistURI) } label: { Label("Play", systemImage: "play.fill") }
                            .buttonStyle(.borderedProminent).controlSize(.large)
                        Button { model.shufflePlay(uris: tracks.map(\.uri)) } label: { Label("Shuffle", systemImage: "shuffle") }
                            .buttonStyle(.bordered).controlSize(.large)
                        Button { model.queueAdd(uri: playlistURI) } label: { Label("Add to Queue", systemImage: "text.append") }
                            .buttonStyle(.bordered).controlSize(.large)
                    }
                }
                Spacer()
            }
            .padding(20)
            Divider()

            if model.library.loadingTracksFor == playlist.id && tracks.isEmpty {
                SkeletonRows()
            } else {
                TrackListView(tracks: tracks)
            }
        }
        .background(.background)
        .navigationTitle(playlist.name)
        .task(id: playlist.id) { await model.library.loadTracks(for: playlist) }
    }
}
