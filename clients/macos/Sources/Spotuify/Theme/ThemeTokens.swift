import SwiftUI

/// Hand-tuned color tokens for the fixed Light and Dark themes. Only the
/// chrome (sidebar, transport wash, global tint) reads from here; the
/// immersive album surfaces (Now Playing, Mini Player, Menu Bar) always use
/// the live `ArtworkPalette` because they are intentionally artwork-driven.
///
/// The chrome is a *neutral warm-tinted* surface that pairs with the editorial
/// Fraunces typography without competing with it. The accent deliberately
/// defers to the system `AccentColor` so a user who has chosen a custom
/// macOS accent still sees it in fixed modes — we only own the surface tone.
struct ThemeTokens: Equatable {
    /// The sidebar / chrome surface tone behind the floating Liquid Glass.
    var chrome: Color
    /// Tinted divider between chrome regions (top of sidebar, top of transport).
    var chromeEdge: Color
    /// Strength of the global accent wash on the transport bar. Tuned to be
    /// visible against the chrome without overpowering it.
    var accentWash: Double
    /// Top-of-bar gradient stop strength.
    var accentEdge: Double

    static let light = ThemeTokens(
        chrome: Color(red: 0.985, green: 0.980, blue: 0.972),
        chromeEdge: Color(red: 0.86, green: 0.85, blue: 0.83).opacity(0.9),
        accentWash: 0.08,
        accentEdge: 0.45)

    static let dark = ThemeTokens(
        chrome: Color(red: 0.085, green: 0.087, blue: 0.095),
        chromeEdge: Color(white: 1.0).opacity(0.08),
        accentWash: 0.12,
        accentEdge: 0.55)

    /// Pick the right set for the active color scheme. Falls back to dark for
    /// the unknown case so uninitialised environments don't render light.
    static func tokens(for scheme: ColorScheme) -> ThemeTokens {
        switch scheme {
        case .light: .light
        case .dark: .dark
        @unknown default: .dark
        }
    }
}
