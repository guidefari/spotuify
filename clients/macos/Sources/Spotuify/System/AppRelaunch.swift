import AppKit
import Foundation

/// Relaunch the app from `url` after the updater swapped the bundle.
/// A detached `sleep && open` outlives this process, so the fresh
/// instance starts after we exit — `NSWorkspace.openApplication` on our
/// own bundle would just re-activate the running (old) instance.
enum AppRelaunch {
    static func relaunch(from url: URL) {
        // Wait for THIS pid to actually exit before `open`: with the
        // old fixed 0.8s sleep, a slow shutdown meant `open` just
        // re-activated the dying old instance and nothing relaunched.
        // Bundle path is passed as $0, not interpolated into the script.
        let pid = ProcessInfo.processInfo.processIdentifier
        let script =
            "for _ in $(seq 1 100); do kill -0 \(pid) 2>/dev/null || break; sleep 0.1; done; "
            + "exec /usr/bin/open \"$0\""
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", script, url.path]
        try? process.run()
        NSApp.terminate(nil)
    }
}
