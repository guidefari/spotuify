import SwiftUI

/// One source of truth for every corner radius used in the app. Names are
/// semantic by purpose, not by pixel value, so a future redesign can retune the
/// scale in one place without touching call sites.
///
/// The scale steps are intentional, not a 1–N progression: each tier earns its
/// place by serving a distinct visual role. If you find yourself needing a new
/// tier, it should replace an existing one or be reused across multiple call
/// sites — never a one-off magic number.
enum RadiusTokens {
    /// 3pt — tiny pill-shaped bars (skeleton text rows).
    static let chip: CGFloat = 3
    /// 6pt — compact row thumbnails: transport cells, mini-player cover,
    /// list-row artwork, history stacked covers.
    static let thumb: CGFloat = 6
    /// 8pt — row hover / selection wash, search field, command pill.
    static let row: CGFloat = 8
    /// 10pt — default chrome surface: settings tiles, device cards, library
    /// header, menu-bar cover, session rows, banners.
    static let chrome: CGFloat = 10
    /// 12pt — grid artwork tile (square album / show cards).
    static let tile: CGFloat = 12
    /// 14pt — hero / detail artwork. The default for `AsyncCoverImage`.
    static let artwork: CGFloat = 14
}
