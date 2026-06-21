import SwiftUI
import SpotuifyKit

/// Which source-sized URL to use for a given surface. Daemon populates
/// three URLs per `MediaItem` (small ≈ 64, default ≈ 300, large ≈ 640+);
/// the consumer picks the one that matches its rendered size so a 40pt
/// thumbnail doesn't fetch a 640px source (5x bandwidth) and a 480pt
/// hero doesn't upscale from 300.
enum CoverImageSize {
    /// 40–50pt row thumbnails (footer, queue rows, history chips,
    /// reminder rows). Falls back to `default` if Spotify only returned
    /// one size.
    case small
    /// 200–300pt list / grid tiles, menu-bar covers, system-media art.
    case `default`
    /// 480pt+ now-playing hero (contained square or full-bleed).
    case large
}

extension MediaItem {
    /// The source-sized URL matching the requested surface tier. Always
    /// returns *some* URL when the item has any art — `default` is the
    /// ground truth — so consumers never have to nil-coalesce.
    func imageURL(for size: CoverImageSize) -> String? {
        switch size {
        case .small: imageURLSmall ?? imageURL
        case .default: imageURL
        case .large: imageURLLarge ?? imageURL
        }
    }
}

/// Loads album artwork from a Spotify CDN URL via `CoverArtCache`, with a
/// graceful placeholder while loading or when missing.
struct AsyncCoverImage: View {
    let url: String?
    var cornerRadius: CGFloat = RadiusTokens.artwork
    /// When true, clips to a perfect `Circle` instead of a rounded rectangle.
    /// Use for artist avatars and any other square source that should read as
    /// round regardless of rendered size — passing a numeric corner radius
    /// would otherwise force you to recompute it per size.
    var isCircle: Bool = false

    @State private var image: NSImage?
    @State private var loadedURL: String?

    var body: some View {
        ZStack {
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
            } else {
                ZStack {
                    Rectangle().fill(.quaternary)
                    Image(systemName: "music.note")
                        .font(.system(size: 28))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .clipShape(clipShape)
        .animation(.easeInOut(duration: 0.25), value: image)
        .task(id: url) {
            guard loadedURL != url else { return }
            image = nil
            loadedURL = url
            image = await CoverArtCache.shared.image(for: url)
        }
    }

    private var clipShape: AnyShape {
        if isCircle {
            AnyShape(Circle())
        } else {
            AnyShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        }
    }
}
