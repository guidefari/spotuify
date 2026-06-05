import Foundation
import UserNotifications
import AppKit
import SpotuifyKit

/// Schedule-ahead macOS notifications for reminders. The daemon is headless and
/// can't post Notification Center alerts, so the GUI app mirrors the daemon's
/// active reminders to local `UNCalendarNotificationTrigger`s (repeating for
/// recurring) — these fire even when the app is later closed. On launch and on
/// every `RemindersChanged`, we cancel our previously-scheduled requests and
/// re-add from the current set. Tapping an action routes back to the daemon.
@MainActor
final class ReminderNotificationScheduler: NSObject, UNUserNotificationCenterDelegate {
    static let shared = ReminderNotificationScheduler()

    private weak var model: AppModel?
    private var configured = false
    private let categoryID = "REMINDER"
    private let prefix = "reminder-"

    func configure(model: AppModel) {
        self.model = model
        guard !configured else { return }
        configured = true

        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
        registerCategory(center)

        // Re-sync when reminders load (each connect) and on every change.
        model.onRemindersReady = { [weak self] in self?.sync() }
        model.addEventObserver { [weak self] event in
            switch event {
            case .remindersChanged, .reminderDue:
                self?.sync()
            default:
                break
            }
        }
    }

    private func registerCategory(_ center: UNUserNotificationCenter) {
        let play = UNNotificationAction(identifier: "PLAY", title: "Play", options: [.foreground])
        let queue = UNNotificationAction(identifier: "QUEUE", title: "Queue", options: [])
        let snooze = UNNotificationAction(identifier: "SNOOZE", title: "Snooze 1h", options: [])
        let dismiss = UNNotificationAction(
            identifier: "DISMISS", title: "Dismiss", options: [.destructive])
        let category = UNNotificationCategory(
            identifier: categoryID, actions: [play, queue, snooze, dismiss],
            intentIdentifiers: [], options: [])
        center.setNotificationCategories([category])
    }

    /// Cancel app-owned requests and reschedule from the daemon's active list.
    func sync() {
        guard let model else { return }
        Task { [weak self] in
            guard let self else { return }
            guard case .reminders(let reminders) = try? await model.request(
                .remindersList(includeInactive: false)) else { return }
            let center = UNUserNotificationCenter.current()
            let existing = await center.pendingNotificationRequests()
            let ours = existing.map(\.identifier).filter { $0.hasPrefix(self.prefix) }
            center.removePendingNotificationRequests(withIdentifiers: ours)
            for reminder in reminders where reminder.state == .active {
                self.schedule(reminder, center: center)
            }
        }
    }

    private func schedule(_ reminder: Reminder, center: UNUserNotificationCenter) {
        let content = UNMutableNotificationContent()
        content.title = "Listen: \(reminder.name)"
        content.body = reminder.message ?? reminder.subtitle
        content.sound = .default
        content.categoryIdentifier = categoryID
        content.userInfo = ["reminder_id": reminder.id, "media_uri": reminder.mediaURI]

        let comps = dateComponents(for: reminder)
        let repeats = reminder.recurrence != .none
        let trigger = UNCalendarNotificationTrigger(dateMatching: comps, repeats: repeats)
        let request = UNNotificationRequest(
            identifier: "\(prefix)\(reminder.id)", content: content, trigger: trigger)
        center.add(request)
    }

    /// Local wall-clock components for the trigger — daily uses hour+minute,
    /// weekly adds weekday, monthly adds day, one-shot pins the full date.
    private func dateComponents(for reminder: Reminder) -> DateComponents {
        let cal = Calendar.current
        let date = reminder.nextDueDate
        switch reminder.recurrence {
        case .daily:
            return cal.dateComponents([.hour, .minute], from: date)
        case .weekly:
            return cal.dateComponents([.weekday, .hour, .minute], from: date)
        case .monthly:
            return cal.dateComponents([.day, .hour, .minute], from: date)
        case .none:
            return cal.dateComponents([.year, .month, .day, .hour, .minute], from: date)
        }
    }

    // MARK: UNUserNotificationCenterDelegate

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        let info = response.notification.request.content.userInfo
        let reminderID = info["reminder_id"] as? String
        let mediaURI = info["media_uri"] as? String
        let action = response.actionIdentifier
        await MainActor.run {
            guard let model = self.model else { return }
            switch action {
            case "PLAY", UNNotificationDefaultActionIdentifier:
                if let mediaURI { model.play(uri: mediaURI) }
                NSApp.activate(ignoringOtherApps: true)
                if let reminderID { model.actLatestNotification(reminderID: reminderID, action: "play") }
            case "QUEUE":
                if let reminderID { model.actLatestNotification(reminderID: reminderID, action: "queue") }
            case "SNOOZE":
                if let reminderID {
                    let until = Int64((Date().timeIntervalSince1970 + 3600) * 1000)
                    model.actLatestNotification(
                        reminderID: reminderID, action: "snooze", snoozeUntilMs: until)
                }
            case "DISMISS":
                if let reminderID { model.actLatestNotification(reminderID: reminderID, action: "dismiss") }
            default:
                break
            }
        }
    }
}
