import Foundation
import Observation

/// Holds reminder schedules + inbox notifications, refreshing on `ReminderDue` /
/// `RemindersChanged` events. The daemon owns the truth; this store renders it.
@MainActor
@Observable
public final class RemindersStore {
    public private(set) var reminders: [Reminder] = []
    public private(set) var notifications: [ReminderNotification] = []
    public private(set) var loading = false

    private weak var model: AppModel?

    public init() {}

    func connect(_ model: AppModel) {
        self.model = model
        model.addEventObserver { [weak self] event in
            guard let self else { return }
            switch event {
            case .reminderDue(let notification):
                self.upsert(notification)
                Task { await self.loadReminders(force: true) }
            case .remindersChanged:
                Task {
                    await self.loadReminders(force: true)
                    await self.loadNotifications(force: true)
                }
            default:
                break
            }
        }
    }

    /// Fetch both reminders + notifications (called on connect/ready).
    public func loadAll() async {
        await loadReminders(force: true)
        await loadNotifications(force: true)
    }

    public func loadReminders(force: Bool = false) async {
        guard let model else { return }
        if !force && !reminders.isEmpty { return }
        loading = true
        defer { loading = false }
        if case .reminders(let result) = try? await model.request(
            .remindersList(includeInactive: false), timeout: .seconds(20)) {
            reminders = result
        }
    }

    public func loadNotifications(force: Bool = false) async {
        guard let model else { return }
        if !force && !notifications.isEmpty { return }
        if case .notifications(let result) = try? await model.request(
            .notificationsList(includeArchived: false), timeout: .seconds(20)) {
            notifications = result
        }
    }

    /// Open (actionable) notifications — what the inbox + badge count care about.
    public var openNotifications: [ReminderNotification] {
        notifications.filter(\.isOpen)
    }

    public var unseenCount: Int {
        notifications.filter { $0.state == .unseen }.count
    }

    private func upsert(_ notification: ReminderNotification) {
        notifications.removeAll { $0.id == notification.id }
        notifications.insert(notification, at: 0)
    }
}
