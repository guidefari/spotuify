import AppKit
import Foundation

/// Relaunch the app from `url` after the updater swapped the bundle.
/// A detached `sleep && open` outlives this process, so the fresh
/// instance starts after we exit — `NSWorkspace.openApplication` on our
/// own bundle would just re-activate the running (old) instance.
enum AppRelaunch {
    static func relaunch(from url: URL) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", "sleep 0.8; /usr/bin/open \"\(url.path)\""]
        try? process.run()
        NSApp.terminate(nil)
    }
}
