import Foundation

/// How often a reminder repeats (mirrors `spotuify_core::Recurrence`).
public enum Recurrence: String, Codable, Sendable, Hashable, CaseIterable {
    case none, daily, weekly, monthly

    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = Recurrence(rawValue: raw) ?? .none
    }

    public var label: String {
        switch self {
        case .none: "One-time"
        case .daily: "Daily"
        case .weekly: "Weekly"
        case .monthly: "Monthly"
        }
    }
}

/// Lifecycle of a reminder schedule.
public enum ReminderState: String, Codable, Sendable, Hashable {
    case active, completed, cancelled
    case other

    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = ReminderState(rawValue: raw) ?? .other
    }
}

/// Lifecycle of a fired notification (inbox occurrence).
public enum NotificationState: String, Codable, Sendable, Hashable {
    case unseen, seen, snoozed, dismissed, done
    case other

    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = NotificationState(rawValue: raw) ?? .other
    }
}

/// A scheduled listening reminder (mirrors `spotuify_core::Reminder`).
public struct Reminder: Codable, Sendable, Hashable, Identifiable {
    public let id: String
    public let mediaURI: String
    public let mediaKind: MediaKind
    public let name: String
    public let subtitle: String
    public let imageURL: String?
    public let anchorAtMs: Int64
    public let recurrence: Recurrence
    public let tz: String
    public let nextDueAtMs: Int64
    public let state: ReminderState
    public let message: String?
    public let createdAtMs: Int64

    enum CodingKeys: String, CodingKey {
        case id
        case mediaURI = "media_uri"
        case mediaKind = "media_kind"
        case name, subtitle
        case imageURL = "image_url"
        case anchorAtMs = "anchor_at_ms"
        case recurrence, tz
        case nextDueAtMs = "next_due_at_ms"
        case state, message
        case createdAtMs = "created_at_ms"
    }

    public var nextDueDate: Date { Date(timeIntervalSince1970: Double(nextDueAtMs) / 1000) }
}

/// A fired reminder occurrence shown in the inbox (mirrors
/// `spotuify_core::Notification`). Named `ReminderNotification` to avoid
/// colliding with `Foundation.Notification`.
public struct ReminderNotification: Codable, Sendable, Hashable, Identifiable {
    public let id: String
    public let reminderID: String
    public let mediaURI: String
    public let mediaKind: MediaKind
    public let name: String
    public let subtitle: String
    public let imageURL: String?
    public let dueAtMs: Int64
    public let firedAtMs: Int64
    public let state: NotificationState
    public let snoozedUntilMs: Int64?
    public let acted: String?
    public let message: String?

    enum CodingKeys: String, CodingKey {
        case id
        case reminderID = "reminder_id"
        case mediaURI = "media_uri"
        case mediaKind = "media_kind"
        case name, subtitle
        case imageURL = "image_url"
        case dueAtMs = "due_at_ms"
        case firedAtMs = "fired_at_ms"
        case state
        case snoozedUntilMs = "snoozed_until_ms"
        case acted, message
    }

    public var dueDate: Date { Date(timeIntervalSince1970: Double(dueAtMs) / 1000) }
    public var isOpen: Bool { state == .unseen || state == .seen || state == .snoozed }
}
