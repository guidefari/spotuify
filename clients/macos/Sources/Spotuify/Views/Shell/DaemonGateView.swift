import SwiftUI
import SpotuifyKit

/// Shown until the daemon is confirmed present AND new enough. Replaces the
/// player UI with actionable install / start / upgrade instructions.
struct DaemonGateView: View {
    @Environment(AppModel.self) private var model
    let readiness: DaemonReadiness

    var body: some View {
        VStack(spacing: 24) {
            Image(systemName: icon)
                .font(.system(size: 52))
                .foregroundStyle(.tint)
                .symbolEffect(.pulse, isActive: isChecking)

            VStack(spacing: 8) {
                Text(title).font(.title.bold()).multilineTextAlignment(.center)
                Text(subtitle)
                    .font(.callout).foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 460)
            }

            if !isChecking {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(Array(commands.enumerated()), id: \.offset) { _, command in
                        CommandRow(command: command)
                    }
                }
                .frame(maxWidth: 520)

                HStack(spacing: 12) {
                    Button {
                        model.forceReconnect()
                    } label: {
                        Label("Retry", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(.borderedProminent)
                    Link("Docs", destination: URL(string: "https://spotuify.vercel.app")!)
                        .buttonStyle(.bordered)
                }
            } else {
                ProgressView().controlSize(.large)
            }
        }
        .padding(40)
        .frame(minWidth: 620, minHeight: 520)
        .background(.background)
    }

    private var isChecking: Bool { readiness == .checking }

    private var icon: String {
        switch readiness {
        case .checking: "antenna.radiowaves.left.and.right"
        case .missing(let installed): installed ? "bolt.horizontal.circle" : "shippingbox"
        case .incompatible: "arrow.up.circle"
        case .ready: "checkmark.circle"
        }
    }

    private var title: String {
        switch readiness {
        case .checking: "Connecting to spotuify…"
        case .missing(let installed): installed ? "The spotuify daemon isn’t running" : "spotuify isn’t installed"
        case .incompatible: "Your spotuify daemon is out of date"
        case .ready: "Connected"
        }
    }

    private var subtitle: String {
        switch readiness {
        case .checking:
            return "Looking for the spotuify daemon."
        case .missing(let installed):
            return installed
                ? "Spotuify found the binary but the daemon didn’t start. Start it, then retry. If it keeps failing, run spotuify doctor."
                : "Spotuify is the backend that plays your music. Install it, then this app connects automatically."
        case .incompatible(let found, let required, let version):
            return "This app needs daemon protocol v\(required), but the running daemon (\(version)) speaks v\(found). Upgrade the daemon and restart it."
        case .ready:
            return ""
        }
    }

    private var commands: [String] {
        switch readiness {
        case .missing(let installed):
            if installed {
                return ["spotuify daemon start", "spotuify doctor"]
            }
            return [
                "brew install planetaryescape/spotuify/spotuify",
                "spotuify daemon start",
            ]
        case .incompatible:
            return [
                "brew upgrade planetaryescape/spotuify/spotuify",
                "spotuify daemon restart",
            ]
        default:
            return []
        }
    }
}

/// A copyable monospaced command line.
private struct CommandRow: View {
    let command: String
    @State private var copied = false

    var body: some View {
        HStack {
            Text(command)
                .font(.system(.callout, design: .monospaced))
                .textSelection(.enabled)
            Spacer(minLength: 12)
            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(command, forType: .string)
                copied = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { copied = false }
            } label: {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help("Copy")
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))
    }
}
