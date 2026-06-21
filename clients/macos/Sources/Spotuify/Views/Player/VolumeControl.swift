import SwiftUI
import SpotuifyKit

/// Volume slider bound to the active device's volume. Commits to the daemon on
/// release; reflects daemon state otherwise.
struct VolumeControl: View {
    @Environment(AppModel.self) private var model
    @State private var dragValue: Double?

    /// Foreground style for the speaker glyph. Defaults to `.secondary` for
    /// chrome surfaces (NowPlayingBar sits on a `.bar` material where
    /// `.secondary` reads fine). Surfaces that sit on a colored or artwork
    /// backdrop (the immersive Now Playing stage) need to pass a palette-aware
    /// style — the default `.secondary` is grey and disappears on a light
    /// cover.
    var iconForegroundStyle: AnyShapeStyle = AnyShapeStyle(HierarchicalShapeStyle.secondary)

    private var deviceVolume: Double {
        Double(model.player.volumePercent ?? 0) / 100.0
    }
    private var shown: Double { dragValue ?? deviceVolume }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 12))
                .foregroundStyle(iconForegroundStyle)
                .frame(width: 16)
            GeometryReader { geo in
                let width = geo.size.width
                ZStack(alignment: .leading) {
                    Capsule().fill(.primary.opacity(OpacityTokens.level15))
                    Capsule().fill(.tint).frame(width: max(0, min(1, shown)) * width)
                }
                .frame(height: 5)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { dragValue = min(1, max(0, $0.location.x / width)) }
                        .onEnded { value in
                            let fraction = min(1, max(0, value.location.x / width))
                            model.setVolume(Int((fraction * 100).rounded()))
                            dragValue = nil
                        }
                )
            }
            .frame(height: 16)
        }
        .disabled(model.player.activeDevice?.supportsVolume == false)
    }

    private var icon: String {
        switch shown {
        case ..<0.01: "speaker.slash.fill"
        case ..<0.34: "speaker.fill"
        case ..<0.67: "speaker.wave.1.fill"
        default: "speaker.wave.2.fill"
        }
    }
}

