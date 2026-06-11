import CryptoKit
import Foundation
import Observation
import os

/// One-click in-app updater. Downloads the release DMG from GitHub,
/// verifies its published SHA-256, mounts it, swaps this app bundle for
/// the new one, and hands back the installed URL so the caller can
/// relaunch. No Sparkle: the release pipeline already publishes
/// `Spotuify-{version}.dmg` + `.sha256` as GitHub release assets, so the
/// updater drives those directly.
@MainActor
@Observable
public final class AppUpdater {
    public enum Phase: Equatable {
        case idle
        case downloading
        case verifying
        case installing
        /// The new bundle is in place; the caller should relaunch.
        case installed(URL)
        case failed(String)

        public var isBusy: Bool {
            switch self {
            case .downloading, .verifying, .installing: return true
            case .idle, .installed, .failed: return false
            }
        }
    }

    public private(set) var phase: Phase = .idle

    private let logger = Logger(subsystem: "com.bhekanik.spotuify", category: "updater")

    public init() {}

    public func reset() { phase = .idle }

    /// Download + verify + install `version`, replacing the running app
    /// bundle. On success `phase == .installed(url)`; the caller
    /// relaunches from `url` and terminates.
    public func install(version: String) async {
        guard !phase.isBusy else { return }
        do {
            let target = try installTarget()
            phase = .downloading
            let asset = "Spotuify-\(version).dmg"
            let base = "https://github.com/planetaryescape/spotuify/releases/download/v\(version)/"
            guard let dmgURL = URL(string: base + asset),
                  let shaURL = URL(string: base + asset + ".sha256")
            else { throw UpdateError.badURL }

            let dmg = try await download(dmgURL, suggestedName: asset)

            phase = .verifying
            let expected = try await fetchExpectedDigest(shaURL)
            let actual = try sha256Hex(of: dmg)
            guard actual == expected else {
                throw UpdateError.checksumMismatch(expected: expected, actual: actual)
            }

            phase = .installing
            let mountPoint = try mountDMG(dmg)
            defer { detachDMG(mountPoint) }
            let newApp = mountPoint.appendingPathComponent("Spotuify.app")
            guard FileManager.default.fileExists(atPath: newApp.path) else {
                throw UpdateError.appMissingFromDMG
            }
            // Stage outside the (about-to-be-detached) DMG volume.
            let staged = try stage(newApp)
            try swapBundle(at: target, with: staged)

            logger.info("updated app bundle at \(target.path) to \(version)")
            phase = .installed(target)
        } catch {
            logger.error("update failed: \(error.localizedDescription)")
            phase = .failed(error.localizedDescription)
        }
    }

    // MARK: - Steps

    private func installTarget() throws -> URL {
        let bundle = Bundle.main.bundleURL
        // Gatekeeper app translocation runs the app from a randomized
        // read-only mount; replacing that path is meaningless. Tell the
        // user to move the app to /Applications first.
        if bundle.path.contains("/AppTranslocation/") {
            throw UpdateError.translocated
        }
        // Don't self-replace Xcode dev builds.
        if bundle.path.contains("/DerivedData/") {
            throw UpdateError.notAnAppBundle(bundle.path)
        }
        guard bundle.pathExtension == "app" else {
            // Dev runs (e.g. from DerivedData) shouldn't self-replace.
            throw UpdateError.notAnAppBundle(bundle.path)
        }
        return bundle
    }

    private func download(_ url: URL, suggestedName: String) async throws -> URL {
        let (temp, response) = try await URLSession.shared.download(from: url)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
            throw UpdateError.downloadFailed(url.lastPathComponent)
        }
        let dest = FileManager.default.temporaryDirectory
            .appendingPathComponent("spotuify-update-\(UUID().uuidString)")
            .appendingPathComponent(suggestedName)
        try FileManager.default.createDirectory(
            at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.moveItem(at: temp, to: dest)
        return dest
    }

    private func fetchExpectedDigest(_ url: URL) async throws -> String {
        let (data, response) = try await URLSession.shared.data(from: url)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200,
              let text = String(data: data, encoding: .utf8),
              // `shasum` format: "<hex>  <filename>"
              let hex = text.split(whereSeparator: \.isWhitespace).first,
              hex.count == 64
        else { throw UpdateError.downloadFailed(url.lastPathComponent) }
        return hex.lowercased()
    }

    private func sha256Hex(of file: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: file)
        defer { try? handle.close() }
        var hasher = SHA256()
        while autoreleasepool(invoking: {
            let chunk = handle.readData(ofLength: 4 * 1024 * 1024)
            guard !chunk.isEmpty else { return false }
            hasher.update(data: chunk)
            return true
        }) {}
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private func mountDMG(_ dmg: URL) throws -> URL {
        let output = try run(
            "/usr/bin/hdiutil",
            ["attach", dmg.path, "-nobrowse", "-readonly", "-plist"])
        guard
            let plist = try PropertyListSerialization.propertyList(
                from: output, options: [], format: nil) as? [String: Any],
            let entities = plist["system-entities"] as? [[String: Any]],
            let mount = entities.compactMap({ $0["mount-point"] as? String }).first
        else { throw UpdateError.mountFailed }
        return URL(fileURLWithPath: mount)
    }

    private func detachDMG(_ mountPoint: URL) {
        _ = try? run("/usr/bin/hdiutil", ["detach", mountPoint.path, "-force"])
    }

    private func stage(_ app: URL) throws -> URL {
        let staged = FileManager.default.temporaryDirectory
            .appendingPathComponent("spotuify-staged-\(UUID().uuidString)")
            .appendingPathComponent("Spotuify.app")
        try FileManager.default.createDirectory(
            at: staged.deletingLastPathComponent(), withIntermediateDirectories: true)
        // `ditto` preserves the code signature, xattrs, and symlinks —
        // FileManager.copyItem historically hasn't been reliable here.
        _ = try run("/usr/bin/ditto", [app.path, staged.path])
        return staged
    }

    /// Replace `target` with `staged`, keeping a rollback copy until the
    /// move of the new bundle succeeds. The running process keeps its
    /// open files, so replacing the bundle under it is safe; the swap
    /// only takes effect at relaunch.
    private func swapBundle(at target: URL, with staged: URL) throws {
        let fm = FileManager.default
        let backup = fm.temporaryDirectory
            .appendingPathComponent("spotuify-backup-\(UUID().uuidString).app")
        try fm.moveItem(at: target, to: backup)
        do {
            // ditto instead of move: the temp dir may sit on another
            // volume, and ditto preserves the signature either way.
            _ = try run("/usr/bin/ditto", [staged.path, target.path])
        } catch {
            try? fm.removeItem(at: target)
            try? fm.moveItem(at: backup, to: target)
            throw error
        }
        try? fm.removeItem(at: backup)
    }

    @discardableResult
    private func run(_ tool: String, _ arguments: [String]) throws -> Data {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        try process.run()
        process.waitUntilExit()
        let output = stdout.fileHandleForReading.readDataToEndOfFile()
        guard process.terminationStatus == 0 else {
            let err = String(
                data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)
            throw UpdateError.toolFailed(
                tool: (tool as NSString).lastPathComponent,
                detail: err?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "")
        }
        return output
    }
}

enum UpdateError: LocalizedError {
    case badURL
    case downloadFailed(String)
    case checksumMismatch(expected: String, actual: String)
    case mountFailed
    case appMissingFromDMG
    case translocated
    case notAnAppBundle(String)
    case toolFailed(tool: String, detail: String)

    var errorDescription: String? {
        switch self {
        case .badURL:
            return "Could not build the download URL."
        case .downloadFailed(let name):
            return "Download failed: \(name). Use the releases page instead."
        case .checksumMismatch:
            return "Downloaded DMG failed checksum verification — not installing."
        case .mountFailed:
            return "Could not mount the downloaded DMG."
        case .appMissingFromDMG:
            return "The DMG didn't contain Spotuify.app."
        case .translocated:
            return "macOS is running this app from a temporary location. Move Spotuify.app to /Applications, relaunch, and try again."
        case .notAnAppBundle(let path):
            return "Not running from an app bundle (\(path)) — update manually."
        case .toolFailed(let tool, let detail):
            return "\(tool) failed: \(detail)"
        }
    }
}
