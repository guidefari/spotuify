import Foundation

/// High-level connection status, surfaced to the UI for banners/spinners.
public enum ConnectionState: Sendable, Equatable {
    case idle
    case connecting
    case ready
    case reconnecting(attempt: Int)
    case failed(String)
}

/// Whether the daemon is present and new enough to drive the app. The UI is
/// gated on this: the player only shows when `.ready`.
public enum DaemonReadiness: Sendable, Equatable {
    /// Still connecting / querying the daemon for the first time.
    case checking
    /// No daemon reachable. `installed` distinguishes "binary missing" from
    /// "binary present but daemon didn't come up".
    case missing(installed: Bool)
    /// Daemon is running but too old for this app's required protocol.
    case incompatible(found: Int, required: Int, version: String)
    /// Daemon present and compatible.
    case ready
}

/// Errors raised by the IPC client layer.
public enum DaemonConnectionError: Error, Sendable, Equatable {
    case socketPathTooLong(String)
    case connectFailed(String)
    case notConnected
    case timeout
    case disconnected
    case unexpectedResponse(String)
}
