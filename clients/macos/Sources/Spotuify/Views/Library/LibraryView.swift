import SwiftUI
import SpotuifyKit

/// Liked Songs — the user's real saved tracks (`/me/tracks`), with filter,
/// sort, and Play-all / Shuffle / Queue-all actions.
struct LikedSongsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let liked = model.library.likedSongs
        NavigationStack {
            Group {
                if model.library.loadingLiked && liked.isEmpty {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if liked.isEmpty {
                    ContentUnavailableView("No liked songs", systemImage: "heart",
                        description: Text("Songs you like on Spotify show up here."))
                } else {
                    TrackListView(tracks: liked, storageKey: "likedLayout") {
                        CollectionHeader(
                            icon: "heart.fill",
                            title: "Liked Songs",
                            subtitle: "\(liked.count) songs",
                            uris: liked.map(\.uri))
                    }
                }
            }
            .mediaDetailDestinations()
        }
        .background(.background)
        .task { await model.library.loadLiked() }
    }
}

/// Saved albums → album detail, as a card grid or list (toggle persisted).
struct AlbumsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 0) {
                EditorialPageHeader("Albums")
                Divider()
                if model.library.loadingAlbums && model.library.savedAlbums.isEmpty {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if model.library.savedAlbums.isEmpty {
                    ContentUnavailableView("No saved albums", systemImage: "square.stack")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    CollectionView(items: model.library.savedAlbums, storageKey: "albumsLayout")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .mediaDetailDestinations()
        }
        .background(.background)
        .task { await model.library.loadAlbums() }
    }
}

/// Followed artists → artist discography, as a card grid or list (toggle persisted).
struct ArtistsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 0) {
                EditorialPageHeader("Artists")
                Divider()
                if model.library.loadingArtists && model.library.followedArtists.isEmpty {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if model.library.followedArtists.isEmpty {
                    ContentUnavailableView("No followed artists", systemImage: "music.mic",
                        description: Text("Artists you follow on Spotify show up here."))
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    CollectionView(
                        items: model.library.followedArtists,
                        storageKey: "artistsLayout", minTile: 150, maxTile: 190)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .mediaDetailDestinations()
        }
        .background(.background)
        .task { await model.library.loadFollowedArtists() }
    }
}

/// A header bar with Play / Shuffle / Queue-all for a collection of tracks.
struct CollectionHeader: View {
    @Environment(AppModel.self) private var model
    let icon: String
    let title: String
    let subtitle: String
    let uris: [String]

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: icon)
                .font(.system(size: 30))
                .foregroundStyle(.tint)
                .frame(width: 56, height: 56)
                .background(.tint.opacity(0.15), in: RoundedRectangle(cornerRadius: 10))
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.displayTitle(26))
                Text(subtitle).font(.callout).foregroundStyle(.secondary)
            }
            Spacer()
            Button { model.playAll(uris: uris) } label: { Label("Play", systemImage: "play.fill") }
                .buttonStyle(.borderedProminent)
            Button { model.shufflePlay(uris: uris) } label: { Label("Shuffle", systemImage: "shuffle") }
                .buttonStyle(.bordered)
            Button { model.queueAll(uris: uris) } label: { Label("Queue All", systemImage: "text.append") }
                .buttonStyle(.bordered)
        }
        .padding(16)
    }
}

/// Square artwork tile for album/show grids. Lifts and deepens its shadow on
/// hover so the cover art reads as the hero of the grid.
struct ArtworkTile: View {
    let item: MediaItem
    @State private var hovering = false

    private var isCircle: Bool { item.kind == .artist }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            AsyncCoverImage(url: item.imageURL, cornerRadius: isCircle ? 200 : Theme.tileCornerRadius)
                .aspectRatio(1, contentMode: .fit)
                .shadow(color: .black.opacity(hovering ? 0.4 : 0.22),
                        radius: hovering ? 18 : 8, y: hovering ? 10 : 4)
                .scaleEffect(hovering ? 1.03 : 1)
            Text(item.name)
                .font(.system(size: 13, weight: .semibold))
                .lineLimit(1)
            if !item.subtitle.isEmpty {
                Text(item.subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            if let meta = item.metaLine {
                Text(meta).font(.caption2).foregroundStyle(.tertiary).lineLimit(1)
            }
        }
        .padding(6)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .animation(.spring(response: 0.3, dampingFraction: 0.7), value: hovering)
    }
}
