import SwiftUI

/// Drop-shadow tokens. All shadows are dark (they have to be — that's what
/// shadows are) so these don't branch on the color scheme. The levels name
/// the visual weight, not a fixed opacity, so the scale can be tuned
/// globally without touching call sites.
struct ShadowTokens: Equatable {
    /// Subtle elevation (hover, list rows).
    var soft: Color
    /// Standard elevation (cards, pills).
    var medium: Color
    /// Pronounced elevation (overlays, popovers).
    var strong: Color
    /// Maximum elevation (deep overlays, banners, tooltips).
    var heavy: Color

    static let `default` = ShadowTokens(
        soft: .black.opacity(OpacityTokens.level22),
        medium: .black.opacity(OpacityTokens.level30),
        strong: .black.opacity(OpacityTokens.level35),
        heavy: .black.opacity(OpacityTokens.level40))
}
