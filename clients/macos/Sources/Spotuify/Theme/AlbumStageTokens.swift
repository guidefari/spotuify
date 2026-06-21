import SwiftUI

/// Color tokens for the immersive Now Playing stage and other album-driven
/// surfaces (Mini Player, Menu Bar header). The album stage is *always* a
/// dark artwork-tinted scrim regardless of the user's chosen color scheme,
/// so these tokens are colour-scheme-agnostic — they don't branch on
/// `.light` / `.dark` like `ThemeTokens` does.
struct AlbumStageTokens: Equatable {
    /// Primary text/icon on the scrim.
    var text: Color
    /// Strong emphasis (eyebrow text, prominent labels).
    var textStrong: Color
    /// Medium emphasis (artist name, secondary labels).
    var textMedium: Color
    /// Muted (time labels).
    var textMuted: Color
    /// Faint (inactive transport icons).
    var textFaint: Color
    /// Dim (unliked heart, decorative icons).
    var textDim: Color
    /// Very faint (empty-state icons, duration text).
    var textVeryFaint: Color
    /// Ghost (inactive lyrics lines, very dim metadata).
    var textGhost: Color

    /// Subtle white wash for glass pill backgrounds and selection states.
    var wash: Color
    /// Hairline white stroke for glass outlines.
    var stroke: Color

    /// Scrim layers (gradient stops over the artwork). Soft = subtle darken;
    /// medium = mid-gradient; heavy = deep darken at the bottom of the popover.
    var scrimSoft: Color
    var scrimMedium: Color
    var scrimHeavy: Color
    var scrimHeavier: Color
    var scrimDeep: Color

    static let `default` = AlbumStageTokens(
        text: .white,
        textStrong: .white.opacity(OpacityTokens.level92),
        textMedium: .white.opacity(OpacityTokens.level80),
        textMuted: .white.opacity(OpacityTokens.level70),
        textFaint: .white.opacity(OpacityTokens.level60),
        textDim: .white.opacity(OpacityTokens.level85),
        textVeryFaint: .white.opacity(OpacityTokens.level50),
        textGhost: .white.opacity(OpacityTokens.level38),
        wash: .white.opacity(OpacityTokens.level12),
        stroke: .white.opacity(OpacityTokens.level08),
        scrimSoft: .black.opacity(OpacityTokens.level40),
        scrimMedium: .black.opacity(OpacityTokens.level55),
        scrimHeavy: .black.opacity(OpacityTokens.level85),
        scrimHeavier: .black.opacity(OpacityTokens.level90),
        scrimDeep: .black.opacity(OpacityTokens.level96))
}
