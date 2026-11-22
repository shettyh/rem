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
    var isCheckingEntitlement: Bool = false
    var isStartingTrial: Bool = false
    var lastSyncedAt: Date?
    var lastSyncError: String?
    var entitlement: EntitlementInfoDto?
    var syncAccessRestricted: Bool = false
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
        static let restrictedAccountId = "rem.ios.restrictedAccountId"
        static let cachedEntitlementAccountId = "rem.ios.entitlement.accountId"
        static let cachedEntitlementState = "rem.ios.entitlement.state"
        static let cachedEntitlementSyncEnabled = "rem.ios.entitlement.syncEnabled"
        static let cachedEntitlementProvider = "rem.ios.entitlement.provider"
        static let cachedEntitlementPlan = "rem.ios.entitlement.plan"
        static let cachedEntitlementExpiresAt = "rem.ios.entitlement.expiresAt"
        static let cachedEntitlementCancelAtPeriodEnd = "rem.ios.entitlement.cancelAtPeriodEnd"
        static let cachedEntitlementCanStartTrial = "rem.ios.entitlement.canStartTrial"
        static let cachedEntitlementTrialDays = "rem.ios.entitlement.trialDays"
        static let cachedEntitlementPriceDisplay = "rem.ios.entitlement.priceDisplay"
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

    var effectiveEntitlementState: EntitlementStateDto {
        if let entitlement {
            return entitlement.state
        }
        if syncAccessRestricted {
            return .none
        }
        return .selfHosted
    }

    var isSyncPaused: Bool {
        guard syncStatus?.signedIn == true else { return false }
        if let entitlement {
            return !entitlement.syncEnabled
        }
        return syncAccessRestricted
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
            if status.signedIn, let accountId = status.accountId {
                if self.entitlement == nil,
                   let cached = Self.loadCachedEntitlement(for: accountId) {
                    self.entitlement = cached
                    self.syncAccessRestricted = !cached.syncEnabled
                } else if self.entitlement == nil,
                          UserDefaults.standard.string(forKey: Keys.restrictedAccountId) == accountId {
                    self.syncAccessRestricted = true
                }
            } else if !status.signedIn {
                self.entitlement = nil
                self.syncAccessRestricted = false
            }
            if !fetchedDecks.isEmpty || status.signedIn {
                self.hasDismissedOnboarding = true
            }
            await rescheduleReminder()
        } catch {
            showToast(error.remDisplayMessage)
        }
        self.isLoading = false
    }

    func refreshAccount(triggerSyncIfUnlocked: Bool = false) async {
        guard syncStatus?.signedIn == true else { return }
        guard !isCheckingEntitlement else { return }
        isCheckingEntitlement = true
        defer { isCheckingEntitlement = false }

        do {
            let profile = try await store.fetchAccount()
            applyAccountProfile(profile)
            if triggerSyncIfUnlocked && !isSyncPaused {
                await syncNow(silent: true)
            }
        } catch {
            // Keep last-known cached entitlement when offline.
        }
    }

    func checkAccessAndRetrySync() async {
        guard syncStatus?.signedIn == true else { return }
        guard !isCheckingEntitlement else { return }
        isCheckingEntitlement = true
        do {
            let profile = try await store.fetchAccount()
            applyAccountProfile(profile)
            isCheckingEntitlement = false
            if !isSyncPaused {
                await syncNow(silent: false)
            } else {
                showToast("Cloud sync is still paused for this account")
            }
        } catch {
            isCheckingEntitlement = false
            showToast(error.remDisplayMessage)
        }
    }

    func startTrial() async {
        guard !isStartingTrial else { return }
        isStartingTrial = true
        defer { isStartingTrial = false }

        do {
            let profile = try await store.startTrial()
            applyAccountProfile(profile)
            lastSyncError = nil
            await syncNow(silent: true)
            showToast("Free trial started · syncing")
        } catch {
            showToast(error.remDisplayMessage)
        }
    }

    func syncNow(silent: Bool = false) async {
        guard let status = syncStatus, status.mode == .cloud, status.signedIn else { return }
        if isSyncPaused && silent { return }
        guard !isSyncing else { return }
        isSyncing = true
        lastSyncError = nil
        do {
            _ = try await store.syncNow()
            lastSyncedAt = Date()
            syncAccessRestricted = false
            UserDefaults.standard.removeObject(forKey: Keys.restrictedAccountId)
            await refreshAll()
            if !silent {
                showToast("Synced with rem Cloud")
            }
        } catch RemFfiError.SubscriptionRequired {
            lastSyncError = nil
            syncAccessRestricted = true
            if let accountId = syncStatus?.accountId {
                UserDefaults.standard.set(accountId, forKey: Keys.restrictedAccountId)
            }
            isSyncing = false
            await refreshAccount(triggerSyncIfUnlocked: false)
            await refreshAll()
            return
        } catch {
            lastSyncError = error.remDisplayMessage
            await refreshAll()
            if !silent {
                showToast("Sync paused: \(error.remDisplayMessage)")
            }
        }
        isSyncing = false
    }

    func applyAccountProfile(_ profile: AccountProfileDto) {
        let resolvedEntitlement = profile.entitlement ?? EntitlementInfoDto(
            state: profile.syncEnabled ? .selfHosted : .none,
            syncEnabled: profile.syncEnabled,
            provider: nil,
            plan: nil,
            expiresAt: nil,
            cancelAtPeriodEnd: false,
            canStartTrial: !profile.syncEnabled,
            trialDays: 30,
            priceDisplay: "$5/year",
            canSubscribePolar: false,
            canManageBilling: false
        )
        entitlement = resolvedEntitlement
        syncAccessRestricted = !resolvedEntitlement.syncEnabled
        if let accountId = syncStatus?.accountId ?? Optional(profile.id) {
            Self.saveCachedEntitlement(resolvedEntitlement, for: accountId)
            if resolvedEntitlement.syncEnabled {
                UserDefaults.standard.removeObject(forKey: Keys.restrictedAccountId)
            } else {
                UserDefaults.standard.set(accountId, forKey: Keys.restrictedAccountId)
            }
        }
    }

    static func saveCachedEntitlement(_ info: EntitlementInfoDto, for accountId: String) {
        let defaults = UserDefaults.standard
        defaults.set(accountId, forKey: Keys.cachedEntitlementAccountId)
        defaults.set(encodeState(info.state), forKey: Keys.cachedEntitlementState)
        defaults.set(info.syncEnabled, forKey: Keys.cachedEntitlementSyncEnabled)
        defaults.set(info.provider, forKey: Keys.cachedEntitlementProvider)
        defaults.set(info.plan, forKey: Keys.cachedEntitlementPlan)
        if let expiresAt = info.expiresAt {
            defaults.set(expiresAt, forKey: Keys.cachedEntitlementExpiresAt)
        } else {
            defaults.removeObject(forKey: Keys.cachedEntitlementExpiresAt)
        }
        defaults.set(info.cancelAtPeriodEnd, forKey: Keys.cachedEntitlementCancelAtPeriodEnd)
        defaults.set(info.canStartTrial, forKey: Keys.cachedEntitlementCanStartTrial)
        defaults.set(Int(info.trialDays), forKey: Keys.cachedEntitlementTrialDays)
        defaults.set(info.priceDisplay, forKey: Keys.cachedEntitlementPriceDisplay)
    }

    static func loadCachedEntitlement(for accountId: String) -> EntitlementInfoDto? {
        let defaults = UserDefaults.standard
        guard defaults.string(forKey: Keys.cachedEntitlementAccountId) == accountId,
              let stateRaw = defaults.string(forKey: Keys.cachedEntitlementState),
              let state = decodeState(stateRaw) else {
            return nil
        }
        let expiresAt: Int64? = defaults.object(forKey: Keys.cachedEntitlementExpiresAt) != nil
            ? Int64(defaults.integer(forKey: Keys.cachedEntitlementExpiresAt))
            : nil
        let trialDaysRaw = defaults.integer(forKey: Keys.cachedEntitlementTrialDays)
        return EntitlementInfoDto(
            state: state,
            syncEnabled: defaults.bool(forKey: Keys.cachedEntitlementSyncEnabled),
            provider: defaults.string(forKey: Keys.cachedEntitlementProvider),
            plan: defaults.string(forKey: Keys.cachedEntitlementPlan),
            expiresAt: expiresAt,
            cancelAtPeriodEnd: defaults.bool(forKey: Keys.cachedEntitlementCancelAtPeriodEnd),
            canStartTrial: defaults.bool(forKey: Keys.cachedEntitlementCanStartTrial),
            trialDays: trialDaysRaw > 0 ? UInt32(trialDaysRaw) : 30,
            priceDisplay: defaults.string(forKey: Keys.cachedEntitlementPriceDisplay) ?? "$5/year",
            canSubscribePolar: false,
            canManageBilling: false
        )
    }

    static func clearCachedEntitlement() {
        let defaults = UserDefaults.standard
        defaults.removeObject(forKey: Keys.restrictedAccountId)
        defaults.removeObject(forKey: Keys.cachedEntitlementAccountId)
        defaults.removeObject(forKey: Keys.cachedEntitlementState)
        defaults.removeObject(forKey: Keys.cachedEntitlementSyncEnabled)
        defaults.removeObject(forKey: Keys.cachedEntitlementProvider)
        defaults.removeObject(forKey: Keys.cachedEntitlementPlan)
        defaults.removeObject(forKey: Keys.cachedEntitlementExpiresAt)
        defaults.removeObject(forKey: Keys.cachedEntitlementCancelAtPeriodEnd)
        defaults.removeObject(forKey: Keys.cachedEntitlementCanStartTrial)
        defaults.removeObject(forKey: Keys.cachedEntitlementTrialDays)
        defaults.removeObject(forKey: Keys.cachedEntitlementPriceDisplay)
    }

    private static func encodeState(_ state: EntitlementStateDto) -> String {
        switch state {
        case .selfHosted: return "self_hosted"
        case .none: return "none"
        case .trialing: return "trialing"
        case .invite: return "invite"
        case .active: return "active"
        case .pastDue: return "past_due"
        case .expired: return "expired"
        }
    }

    private static func decodeState(_ raw: String) -> EntitlementStateDto? {
        switch raw {
        case "self_hosted": return .selfHosted
        case "none": return .none
        case "trialing": return .trialing
        case "invite": return .invite
        case "active": return .active
        case "past_due": return .pastDue
        case "expired": return .expired
        default: return nil
        }
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
            showToast(error.remDisplayMessage)
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
            showToast(error.remDisplayMessage)
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
        entitlement = nil
        syncAccessRestricted = false
        Self.clearCachedEntitlement()
        await refreshAll()
        await refreshAccount(triggerSyncIfUnlocked: false)
        if !isSyncPaused {
            await syncNow(silent: true)
            if !isSyncPaused && lastSyncError == nil {
                showToast("Signed in · synced")
            }
        }
    }

    func signOut(silent: Bool = false) async {
        do {
            try await store.signOut()
            lastSyncError = nil
            lastSyncedAt = nil
            entitlement = nil
            syncAccessRestricted = false
            Self.clearCachedEntitlement()
            await refreshAll()
            if !silent {
                showToast("Signed out · local cards kept")
            }
        } catch {
            showToast(error.remDisplayMessage)
        }
    }

    func deleteCloudAccount() async {
        do {
            try await store.deleteCloudAccount()
            lastSyncError = nil
            lastSyncedAt = nil
            entitlement = nil
            syncAccessRestricted = false
            Self.clearCachedEntitlement()
            await refreshAll()
            showToast("Cloud account deleted · local cards kept")
        } catch {
            showToast(error.remDisplayMessage)
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
