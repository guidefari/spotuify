import SwiftUI

/// Shared visual constants and small reusable styles. Cover-derived color lives
/// in `ArtworkPalette`/`ArtworkTheme`; the editorial type tier lives in
/// `EditorialFont` (Fraunces); corner radii live in `RadiusTokens`. This holds
/// the static layout tokens.
enum Theme {
    static let sidebarWidth: CGFloat = 212
    static let nowPlayingBarHeight: CGFloat = 92

    enum TrackColumn {
        static let artwork: CGFloat = 40
        static let album: CGFloat = 220
        static let dateAdded: CGFloat = 90
        static let actions: CGFloat = 100
        static let duration: CGFloat = 48
    }

    static func timeString(_ ms: UInt64) -> String {
        let totalSeconds = Int(ms / 1000)
        return String(format: "%d:%02d", totalSeconds / 60, totalSeconds % 60)
    }
}

/// Standard editorial page title (Fraunces display) with an optional trailing
/// accessory — used at the top of every destination for a consistent magazine
/// masthead feel.
struct EditorialPageHeader<Trailing: View>: View {
    let title: String
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .font(.displayTitle(30))
                .foregroundStyle(.primary)
            Spacer()
            trailing()
        }
        .padding(.horizontal, 20)
        .padding(.top, 18)
        .padding(.bottom, 12)
    }
}

/// Convenience initialiser for a trailing-less `EditorialPageHeader`.
extension EditorialPageHeader where Trailing == EmptyView {
    init(_ title: String) { self.init(title: title, trailing: { EmptyView() }) }
}

extension View {
    /// Capsule Liquid Glass treatment for search / filter inputs.
    func glassField() -> some View {
        padding(.horizontal, 12)
            .padding(.vertical, 8)
            .glassEffect(.regular.interactive(), in: .capsule)
    }

    /// A small Fraunces section heading for grouped lists.
    func editorialSectionHeader() -> some View {
        font(.displayTitle(17))
    }
}

extension View {
    /// Subtle hover-highlightable row used in lists.
    func selectableRowBackground(_ selected: Bool) -> some View {
        background {
            RoundedRectangle(cornerRadius: RadiusTokens.row)
                .fill(selected ? AnyShapeStyle(.tint.opacity(OpacityTokens.level18)) : AnyShapeStyle(.clear))
        }
    }
}

/// A transport icon button with a consistent hit area and hover feel.
struct TransportButton: View {
    @Environment(ArtworkTheme.self) private var theme
    let systemName: String
    var size: CGFloat = 16
    var prominent: Bool = false
    let action: () -> Void

    @State private var hovering = false

    private var palette: ArtworkPalette { theme.palette }

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: size, weight: .semibold))
                .frame(width: prominent ? 44 : 32, height: prominent ? 44 : 32)
                .background {
                    if prominent {
                        // Prominent play button: brand tint on a dark palette,
                        // dark surface on a light palette. The dark surface
                        // keeps the button readable when the wash is a pastel
                        // cover accent (otherwise the green fill blends in).
                        Circle().fill(palette.isLight ? AnyShapeStyle(palette.background) : AnyShapeStyle(.tint))
                    } else {
                        Circle().fill(hovering ? AnyShapeStyle(.primary.opacity(OpacityTokens.level08)) : AnyShapeStyle(.clear))
                    }
                }
                .foregroundStyle(prominent
                                 ? AnyShapeStyle(palette.isLight ? Color.white : AlbumStageTokens.default.text)
                                 : AnyShapeStyle(palette.primary))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}
