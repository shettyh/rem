import Foundation
import UserNotifications

/// Schedules a quiet local daily reminder that only fires when cards are due.
enum ReminderManager {
    private static let notificationId = "com.shettyh.rem.daily-reminder"

    static func requestPermission() async -> Bool {
        let center = UNUserNotificationCenter.current()
        do {
            return try await center.requestAuthorization(options: [.alert, .sound, .badge])
        } catch {
            return false
        }
    }

    static func syncSchedule(
        enabled: Bool,
        hour: Int,
        minute: Int,
        dueCount: UInt32,
        estimatedMinutes: UInt32
    ) async {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: [notificationId])

        guard enabled else { return }

        let settings = await center.notificationSettings()
        guard settings.authorizationStatus == .authorized
                || settings.authorizationStatus == .provisional
        else { return }

        let content = UNMutableNotificationContent()
        content.title = "rem"
        if dueCount > 0 {
            let cardWord = dueCount == 1 ? "card" : "cards"
            content.body = "\(dueCount) \(cardWord) due · ~\(max(1, estimatedMinutes)) min session."
        } else {
            // Schedule for tomorrow's due cards if today is already cleared
            content.body = "Time for a quick review session."
        }
        content.sound = .default

        var components = DateComponents()
        components.hour = hour
        components.minute = minute

        // If today is already caught up (dueCount == 0), schedule starting tomorrow so we never nag today.
        let trigger: UNNotificationTrigger
        if dueCount == 0 {
            let calendar = Calendar.current
            if let tomorrow = calendar.date(byAdding: .day, value: 1, to: Date()) {
                var nextDay = calendar.dateComponents([.year, .month, .day], from: tomorrow)
                nextDay.hour = hour
                nextDay.minute = minute
                trigger = UNCalendarNotificationTrigger(dateMatching: nextDay, repeats: false)
            } else {
                trigger = UNCalendarNotificationTrigger(dateMatching: components, repeats: true)
            }
        } else {
            trigger = UNCalendarNotificationTrigger(dateMatching: components, repeats: true)
        }

        let request = UNNotificationRequest(
            identifier: notificationId,
            content: content,
            trigger: trigger
        )
        try? await center.add(request)
    }
}
