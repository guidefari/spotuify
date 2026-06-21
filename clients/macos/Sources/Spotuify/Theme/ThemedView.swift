import SwiftUI

/// Window-level wrapper that applies the user's `ThemePreference`:
/// - `.preferredColorScheme(...)` for light / dark / system / adaptive.
/// - `.tint(...)` so chrome surfaces inherit a coherent accent.
///
/// The `usesArtworkAccent` flag controls whether the main window's adaptive
/// mode tints with the live `ArtworkPalette` accent. Windows that don't show
/// the album stage (Settings) pass `false` and always use the system accent;
/// windows that *are* the album stage (Mini Player, Menu Bar) intentionally
/// bypass this wrapper and pin their own `.tint(theme.accent)`.
struct ThemedView<Content: View>: View {
    @AppStorage(ThemePreference.storageKey) private var preference: ThemePreference = .system
    @Environment(ArtworkTheme.self) private var artworkTheme
    let usesArtworkAccent: Bool
    @ViewBuilder let content: () -> Content

    private var tint: Color {
        if usesArtworkAccent && preference.isAdaptive {
            artworkTheme.accent
        } else {
            Color("AccentColor")
        }
    }

    var body: some View {
        content()
            .tint(tint)
            .preferredColorScheme(preference.colorScheme)
    }
}
