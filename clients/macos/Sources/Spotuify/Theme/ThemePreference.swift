import SwiftUI

/// User-chosen appearance. Persisted in `UserDefaults` under
/// `themePreference` and read independently by the settings pane and any
/// chrome surface that should respect the theme.
///
/// - `system`: follow the OS appearance. Fixed chrome, system accent.
/// - `light`: force light color scheme, regardless of OS preference.
/// - `dark`: force dark color scheme, regardless of OS preference.
/// - `adaptive`: the original artwork-driven look. The sidebar / now-playing
///   bar wash, the global `.tint`, and the immersive album surfaces all flow
///   from the current artwork palette. This is what shipped before the
///   preference existed and must remain visually unchanged.
enum ThemePreference: String, CaseIterable, Identifiable {
    case system, light, dark, adaptive
    var id: String { rawValue }

    /// SwiftUI color scheme the app should force. `nil` means "follow the
    /// system / don't override", which is the correct answer for both
    /// `.system` and `.adaptive` (the latter wants no scheme override either,
    /// since the album surfaces are designed to look right against either).
    var colorScheme: ColorScheme? {
        switch self {
        case .system, .adaptive: nil
        case .light: .light
        case .dark: .dark
        }
    }

    /// True when the chrome should wash itself with the current artwork
    /// palette. The album surfaces (Now Playing, Mini Player, Menu Bar)
    /// always do, regardless of preference — only the chrome respects the
    /// theme switch.
    var isAdaptive: Bool { self == .adaptive }

    var displayName: String {
        switch self {
        case .system: "Follow system"
        case .light: "Light"
        case .dark: "Dark"
        case .adaptive: "Adaptive"
        }
    }

    var explanation: String {
        switch self {
        case .system:
            "Match your Mac's appearance. Sidebar and chrome use a fixed look; the album stage still adapts to the playing track."
        case .light:
            "Force a light appearance, regardless of your Mac's setting. The album stage still adapts to the playing track."
        case .dark:
            "Force a dark appearance, regardless of your Mac's setting. The album stage still adapts to the playing track."
        case .adaptive:
            "The original look. Sidebar, transport, and the global tint shift with the album artwork."
        }
    }

    /// `UserDefaults` key shared with the chrome surfaces that read the
    /// preference directly via `@AppStorage`.
    static let storageKey = "themePreference"
}
