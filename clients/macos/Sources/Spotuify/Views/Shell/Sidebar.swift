import SwiftUI
import SpotuifyKit

/// The navigation sidebar — a native `List` so it adopts Tahoe's floating
/// Liquid Glass automatically. The Fraunces wordmark and connection badge are
/// pinned as top/bottom insets.
struct Sidebar: View {
    @Environment(AppModel.self) private var model
    @Environment(ArtworkTheme.self) private var theme
    @Environment(\.colorScheme) private var colorScheme
    @AppStorage(ThemePreference.storageKey) private var themePreference: ThemePreference = .system
    @Binding var selection: Destination

    private var tokens: ThemeTokens { ThemeTokens.tokens(for: colorScheme) }

    var body: some View {
        List(selection: selectionBinding) {
            ForEach(Destination.allCases) { destination in
                Label(destination.title, systemImage: destination.icon)
                    .tag(destination)
            }
        }
        .listStyle(.sidebar)
        // Adaptive: wash the sidebar with the cover's background colour so the
        // whole chrome shares the now-playing mood. Fixed themes: use a
        // hand-tuned chrome tone that matches the active color scheme so the
        // sidebar doesn't fight the rest of the surface.
        .scrollContentBackground(.hidden)
        .background { sidebarBackground }
        .animation(.easeInOut(duration: 0.6), value: themePreference)
        .safeAreaInset(edge: .top, spacing: 0) {
            Text("spotuify")
                .font(.displayTitle(22))
                .foregroundStyle(.tint)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 18)
                .padding(.top, 10)
                .padding(.bottom, 6)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            connectionRow
        }
    }

    /// Sidebar single-selection wants an optional binding; never clear to nil.
    private var selectionBinding: Binding<Destination?> {
        Binding(get: { selection }, set: { if let value = $0 { selection = value } })
    }

    /// Adaptive = artwork wash (the original look). Fixed themes = solid
    /// hand-tuned chrome tone so the sidebar sits in the active color scheme
    /// without picking up album hues.
    @ViewBuilder
    private var sidebarBackground: some View {
        if themePreference.isAdaptive {
            LinearGradient(
                colors: [theme.background.opacity(0.95), theme.background.opacity(0.6)],
                startPoint: .top, endPoint: .bottom)
                .animation(.easeInOut(duration: 0.6), value: theme.background)
                .ignoresSafeArea()
        } else {
            tokens.chrome.ignoresSafeArea()
        }
    }

    private var connectionRow: some View {
        HStack(spacing: 6) {
            Circle().fill(badgeColor).frame(width: 7, height: 7)
            Text(badgeText).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 10)
    }

    private var badgeColor: Color {
        switch model.connectionState {
        case .ready: .green
        case .connecting, .reconnecting: .yellow
        case .failed: .red
        case .idle: .gray
        }
    }

    private var badgeText: String {
        switch model.connectionState {
        case .idle: "Starting…"
        case .connecting: "Connecting…"
        case .reconnecting(let n): "Reconnecting (\(n))…"
        case .ready: "Connected"
        case .failed: "Daemon offline"
        }
    }
}
