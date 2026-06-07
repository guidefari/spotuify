import SwiftUI
import SpotuifyKit

/// Subscribed podcasts grid → show detail (episodes, unplayed filter, sort).
struct PodcastsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 0) {
                EditorialPageHeader("Podcasts")
                Divider()
                if model.library.loadingShows && model.library.savedShows.isEmpty {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if model.library.savedShows.isEmpty {
                    ContentUnavailableView("No podcasts", systemImage: "mic",
                        description: Text("Shows you follow on Spotify appear here."))
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    CollectionView(items: model.library.savedShows, storageKey: "podcastsLayout")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .mediaDetailDestinations()
        }
        .background(.background)
        .task { await model.library.loadShows() }
    }
}
