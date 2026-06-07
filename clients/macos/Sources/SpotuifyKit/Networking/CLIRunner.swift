import Foundation

/// Runs the bundled `spotuify` CLI for capabilities exposed only through the CLI
/// (today: visual config editing via `config show`/`config set` + `reload`).
/// Resolves the same binary `DaemonLauncher` uses and inherits the app's
/// environment, so it reads/writes the config for the active instance — the
/// same one the daemon was launched with.
public enum CLIRunner {
    public struct CLIError: Error, Sendable {
        public let message: String
    }

    /// Run `spotuify <args>` and return stdout. Throws if the binary can't be
    /// resolved or the process exits non-zero. Bounded by `timeout` seconds.
    @discardableResult
    public static func run(_ args: [String], timeout: TimeInterval = 15) async throws -> String {
        guard let binary = DaemonLauncher.resolveBinary() else {
            throw CLIError(message: "spotuify binary not found on PATH")
        }
        return try await Task.detached(priority: .utility) {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: binary)
            process.arguments = args
            let stdout = Pipe()
            process.standardOutput = stdout
            process.standardError = Pipe()
            do {
                try process.run()
            } catch {
                throw CLIError(message: "failed to launch spotuify: \(error.localizedDescription)")
            }
            // Bounded wait: terminate if the command overruns.
            let deadline = Date().addingTimeInterval(timeout)
            while process.isRunning && Date() < deadline {
                usleep(40_000)
            }
            if process.isRunning {
                process.terminate()
                throw CLIError(message: "spotuify \(args.first ?? "") timed out")
            }
            let data = stdout.fileHandleForReading.readDataToEndOfFile()
            let output = String(data: data, encoding: .utf8) ?? ""
            if process.terminationStatus != 0 {
                throw CLIError(
                    message: "spotuify \(args.joined(separator: " ")) exited \(process.terminationStatus)")
            }
            return output
        }.value
    }
}
