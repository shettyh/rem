import Foundation
import Observation
import SwiftUI

enum RemTab: Hashable {
    case today
    case decks
    case stats
    case settings
}

struct StudyLaunchRequest: Identifiable {
    let id = UUID()
    let deckId: String?
}

struct CaptureSheetRequest: Identifiable {
    let id = UUID()
    let deckId: String?
    let editingCard: CardDto?
    let onSaved: (() -> Void)?
}

@Observable
@MainActor
final class AppState {
    let store: RemStore
    private let authManager = AuthSessionManager()

    var selectedTab: RemTab = .today
    var activeDeckId: String?

    var todaySummary: TodaySummaryDto?
    var decks: [DeckDto] = []
    var syncStatus: SyncStatusDto?
    var storageSummary: StorageSummaryDto?

    var isLoading: Bool = true
    var isSyncing: Bool = false
    var isAuthenticating: Bool = false
    var lastSyncedAt: Date?
    var lastSyncError: String?
    var toastMessage: String?

    var pendingAccountSwitch: PendingAccountSwitch?
    var expandCustomServerInSettings: Bool = false

    var studyLaunch: StudyLaunchRequest?
    var captureRequest: CaptureSheetRequest?
    var showNewDeckSheet: Bool = false

    var hasDismissedOnboarding: Bool {
        didSet { UserDefaults.standard.set(hasDismissedOnboarding, forKey: Keys.dismissedOnboarding) }
    }
    var cardTextSize: CardTextSize {
        didSet { UserDefaults.standard.set(cardTextSize.rawValue, forKey: Keys.cardTextSize) }
    }
    var appearance: AppAppearance {
        didSet { UserDefaults.standard.set(appearance.rawValue, forKey: Keys.appearance) }
    }
    var hapticsEnabled: Bool {
        didSet { UserDefaults.standard.set(hapticsEnabled, forKey: Keys.hapticsEnabled) }
    }
    var reminderEnabled: Bool {
        didSet { UserDefaults.standard.set(reminderEnabled, forKey: Keys.reminderEnabled) }
    }
    var reminderHour: Int {
        didSet { UserDefaults.standard.set(reminderHour, forKey: Keys.reminderHour) }
    }
    var reminderMinute: Int {
        didSet { UserDefaults.standard.set(reminderMinute, forKey: Keys.reminderMinute) }
    }

    private enum Keys {
        static let dismissedOnboarding = "rem.ios.dismissedOnboarding"
        static let cardTextSize = "rem.ios.cardTextSize"
        static let appearance = "rem.ios.appearance"
        static let hapticsEnabled = "rem.ios.hapticsEnabled"
        static let reminderEnabled = "rem.ios.reminderEnabled"
        static let reminderHour = "rem.ios.reminderHour"
        static let reminderMinute = "rem.ios.reminderMinute"
    }

    private var toastTask: Task<Void, Never>?

    init(store: RemStore) {
        self.store = store
        let defaults = UserDefaults.standard
        self.hasDismissedOnboarding = defaults.bool(forKey: Keys.dismissedOnboarding)
        self.cardTextSize = CardTextSize(rawValue: defaults.string(forKey: Keys.cardTextSize) ?? "") ?? .default
        self.appearance = AppAppearance(rawValue: defaults.string(forKey: Keys.appearance) ?? "") ?? .system
        self.hapticsEnabled = defaults.object(forKey: Keys.hapticsEnabled) == nil ? true : defaults.bool(forKey: Keys.hapticsEnabled)
        self.reminderEnabled = defaults.bool(forKey: Keys.reminderEnabled)
        self.reminderHour = defaults.object(forKey: Keys.reminderHour) == nil ? 20 : defaults.integer(forKey: Keys.reminderHour)
        self.reminderMinute = defaults.object(forKey: Keys.reminderMinute) == nil ? 30 : defaults.integer(forKey: Keys.reminderMinute)
    }

    var shouldShowFirstRun: Bool {
        !isLoading
            && !hasDismissedOnboarding
            && decks.isEmpty
            && (syncStatus?.signedIn != true)
    }

    var defaultCaptureDeckId: String? {
        if let activeDeckId, decks.contains(where: { $0.id == activeDeckId }) {
            return activeDeckId
        }
        return decks.first?.id
    }

    var reminderDate: Date {
        get {
            var comps = DateComponents()
            comps.hour = reminderHour
            comps.minute = reminderMinute
            return Calendar.current.date(from: comps) ?? Date()
        }
        set {
            let comps = Calendar.current.dateComponents([.hour, .minute], from: newValue)
            reminderHour = comps.hour ?? 20
            reminderMinute = comps.minute ?? 30
            Task { await rescheduleReminder() }
        }
    }

    func refreshAll() async {
        do {
            async let summaryTask = store.getTodaySummary()
            async let decksTask = store.listDecks()
            async let syncTask = store.syncStatus()
            async let storageTask = store.getStorageSummary()

            let (summary, fetchedDecks, status, storage) = try await (
                summaryTask, decksTask, syncTask, storageTask
            )
            self.todaySummary = summary
            self.decks = fetchedDecks
            self.syncStatus = status
            self.storageSummary = storage
            if !fetchedDecks.isEmpty || status.signedIn {
                self.hasDismissedOnboarding = true
            }
            await rescheduleReminder()
        } catch {
            showToast(error.localizedDescription)
        }
        self.isLoading = false
    }

    func syncNow(silent: Bool = false) async {
        guard let status = syncStatus, status.mode == .cloud, status.signedIn else { return }
        guard !isSyncing else { return }
        isSyncing = true
        lastSyncError = nil
        do {
            _ = try await store.syncNow()
            lastSyncedAt = Date()
            await refreshAll()
            if !silent {
                showToast("Synced with rem Cloud")
            }
        } catch {
            lastSyncError = error.localizedDescription
            await refreshAll()
            if !silent {
                showToast("Sync paused: \(error.localizedDescription)")
            }
        }
        isSyncing = false
    }

    // MARK: - Cloud OAuth & Account Lifecycle

    func signIn(provider: OAuthProviderDto, customServerUrl: String? = nil) async {
        guard !isAuthenticating else { return }
        isAuthenticating = true
        defer { isAuthenticating = false }

        let targetUrl: String
        if let custom = customServerUrl?.trimmingCharacters(in: .whitespacesAndNewlines), !custom.isEmpty {
            targetUrl = custom
        } else {
            targetUrl = await store.defaultCloudServerUrl()
        }

        do {
            let oauthStart = try await store.beginOauth(serverUrl: targetUrl, provider: provider)
            let code = try await authManager.authenticate(
                authorizeUrlString: oauthStart.authorizeUrl,
                expectedState: oauthStart.state
            )
            try await completeOAuth(
                serverUrl: targetUrl,
                code: code,
                pkceVerifier: oauthStart.pkceVerifier,
                resetExistingBinding: false
            )
        } catch OAuthFlowError.cancelled {
            return
        } catch let RemFfiError.AccountMismatch(boundServerUrl, boundAccountId, _, incomingAccountId) {
            // Note: OAuth code was already consumed on exchange; prompt user to re-authenticate with reset.
            pendingAccountSwitch = PendingAccountSwitch(
                serverUrl: targetUrl,
                code: "",
                pkceVerifier: "",
                boundServerUrl: boundServerUrl,
                boundAccountId: boundAccountId,
                incomingAccountId: incomingAccountId
            )
        } catch {
            showToast(error.localizedDescription)
        }
    }

    func switchAccountWithReauth(provider: OAuthProviderDto, serverUrl: String) async {
        pendingAccountSwitch = nil
        guard !isAuthenticating else { return }
        isAuthenticating = true
        defer { isAuthenticating = false }

        do {
            let oauthStart = try await store.beginOauth(serverUrl: serverUrl, provider: provider)
            let code = try await authManager.authenticate(
                authorizeUrlString: oauthStart.authorizeUrl,
                expectedState: oauthStart.state
            )
            try await completeOAuth(
                serverUrl: serverUrl,
                code: code,
                pkceVerifier: oauthStart.pkceVerifier,
                resetExistingBinding: true
            )
        } catch OAuthFlowError.cancelled {
            return
        } catch {
            showToast(error.localizedDescription)
        }
    }

    private func completeOAuth(
        serverUrl: String,
        code: String,
        pkceVerifier: String,
        resetExistingBinding: Bool
    ) async throws {
        _ = try await store.finishOauth(
            serverUrl: serverUrl,
            code: code,
            pkceVerifier: pkceVerifier,
            resetExistingBinding: resetExistingBinding
        )
        hasDismissedOnboarding = true
        lastSyncError = nil
        await refreshAll()
        showToast("Signed in · syncing collection")
        await syncNow(silent: true)
    }

    func signOut() async {
        do {
            try await store.signOut()
            lastSyncError = nil
            lastSyncedAt = nil
            await refreshAll()
            showToast("Signed out · local cards kept")
        } catch {
            showToast(error.localizedDescription)
        }
    }

    func deleteCloudAccount() async {
        do {
            try await store.deleteCloudAccount()
            lastSyncError = nil
            lastSyncedAt = nil
            await refreshAll()
            showToast("Cloud account deleted · local cards kept")
        } catch {
            showToast(error.localizedDescription)
        }
    }

    // MARK: - Reminders & Backup Export

    func setReminderEnabled(_ enabled: Bool) async {
        if enabled {
            let granted = await ReminderManager.requestPermission()
            if !granted {
                reminderEnabled = false
                showToast("Enable notifications in iOS Settings for daily reminders")
                return
            }
        }
        reminderEnabled = enabled
        await rescheduleReminder()
    }

    func rescheduleReminder() async {
        await ReminderManager.syncSchedule(
            enabled: reminderEnabled,
            hour: reminderHour,
            minute: reminderMinute,
            dueCount: todaySummary?.totalDue ?? 0,
            estimatedMinutes: todaySummary?.estimatedMinutes ?? 0
        )
    }

    func exportBackupFile() async -> URL? {
        do {
            let json = try await store.exportBackupJson()
            let formatter = DateFormatter()
            formatter.dateFormat = "yyyy-MM-dd"
            let stamp = formatter.string(from: Date())
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("rem-backup-\(stamp).json")
            try json.write(to: url, atomically: true, encoding: .utf8)
            return url
        } catch {
            showToast("Backup failed: \(error.localizedDescription)")
            return nil
        }
    }

    // MARK: - Decks, Capture & Study

    func createDeck(name: String, color: String) async -> DeckDto? {
        do {
            let deck = try await store.createDeck(name: name, color: color)
            hasDismissedOnboarding = true
            await refreshAll()
            showToast("Deck \"\(deck.name)\" created")
            Task { await syncNow(silent: true) }
            return deck
        } catch {
            showToast(error.localizedDescription)
            return nil
        }
    }

    func deleteDeck(id: String) async {
        do {
            try await store.deleteDeck(id: id)
            await refreshAll()
            showToast("Deck deleted")
            Task { await syncNow(silent: true) }
        } catch {
            showToast(error.localizedDescription)
        }
    }

    func openQuickCapture(deckId: String? = nil, editingCard: CardDto? = nil, onSaved: (() -> Void)? = nil) {
        if decks.isEmpty && editingCard == nil {
            showNewDeckSheet = true
            showToast("Create a deck first to add cards")
            return
        }
        captureRequest = CaptureSheetRequest(
            deckId: deckId ?? editingCard?.deckId ?? defaultCaptureDeckId,
            editingCard: editingCard,
            onSaved: onSaved
        )
    }

    func startStudy(deckId: String? = nil) {
        studyLaunch = StudyLaunchRequest(deckId: deckId)
    }

    func showToast(_ message: String) {
        toastTask?.cancel()
        toastMessage = message
        toastTask = Task {
            try? await Task.sleep(for: .seconds(2.6))
            guard !Task.isCancelled else { return }
            toastMessage = nil
        }
    }
}
