import Foundation

/// Namespace for the spotuify macOS client library. The kit holds all the
/// testable, non-UI logic (IPC framing, wire models, stores, system bridges)
/// so it can be unit-tested without launching the app.
public enum SpotuifyKit {
    /// Minimum daemon IPC protocol version this client requires. Mirrors
    /// `spotuify_protocol::IPC_PROTOCOL_VERSION`. The app gates its UI on the
    /// running daemon reporting `protocol_version >= ipcProtocolVersion`, so a
    /// stale daemon can't break the new features (v3 = listening reminders;
    /// v4 = artist discography browser — `followed-artists` + `album_group`/
    /// `in_library` on `MediaItem`).
    public static let ipcProtocolVersion = 4
}
