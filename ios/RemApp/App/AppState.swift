import Foundation
import Observation
import SwiftUI

enum RemTab: Hashable {
    case today
    case decks
    case stats
    case settings
}

struct CustomStudySpec {
    let mode: CustomStudyModeDto
    let amount: UInt32
}

struct StudyLaunchRequest: Identifiable {
    let id = UUID()
    let deckId: String?
    let customStudy: CustomStudySpec?

    init(deckId: String?, customStudy: CustomStudySpec? = nil) {
        self.deckId = deckId
        self.customStudy = customStudy
    }
}

struct PendingBackupImport {
    let json: String
    let preview: BackupPreviewDto
}

struct PendingApkgImport {
    let tempFileURL: URL
    let preview: ApkgImportPreviewDto
}

enum DeckImportKind: Equatable {
    case backup
    case apkg
}

struct CaptureSheetRequest: Identifiable {
    let id = UUID()
    let deckId: String?
    let editingCard: CardDto?
    let onSaved: (() -> Void)?
}

struct DeckEditRequest: Identifiable {
    let id = UUID()
    let deckId: String
    let onDeleted: (() -> Void)?
}

@Observable
@MainActor
final class AppState {
    let store: RemStore
    let storeKit = StoreKitManager()
    private let authManager = AuthSessionManager()

    var selectedTab: RemTab = .today
    var activeDeckId: String?
    var collapsedDeckPaths: Set<String> = Set(
        UserDefaults.standard.stringArray(forKey: "rem.collapsedDeckPaths") ?? []
    )

    func toggleDeckCollapse(_ path: String) {
        if collapsedDeckPaths.contains(path) {
            collapsedDeckPaths.remove(path)
        } else {
            collapsedDeckPaths.insert(path)
        }
        UserDefaults.standard.set(Array(collapsedDeckPaths), forKey: "rem.collapsedDeckPaths")
    }

    func pruneCollapsedDeckPaths(removing removedPaths: [String]) {
        guard !removedPaths.isEmpty else { return }
        let next = DeckTreeBuilder.pruneCollapsedPaths(collapsedDeckPaths, removing: removedPaths)
        if next != collapsedDeckPaths {
            collapsedDeckPaths = next
            UserDefaults.standard.set(Array(next), forKey: "rem.collapsedDeckPaths")
        }
    }

    var todaySummary: TodaySummaryDto?
    var decks: [DeckDto] = []
    var syncStatus: SyncStatusDto?
    var storageSummary: StorageSummaryDto?

    var isLoading: Bool = true
    var isSyncing: Bool = false
    var isAuthenticating: Bool = false
    var isCheckingEntitlement: Bool = false
    var isStartingTrial: Bool = false
    var isImportingBackup: Bool = false
    var isImportingApkg: Bool = false
    var lastSyncedAt: Date?
    var lastSyncError: String?
    var entitlement: EntitlementInfoDto?
    var syncAccessRestricted: Bool = false
    var toastMessage: String?

    var pendingAccountSwitch: PendingAccountSwitch?
    var expandCustomServerInSettings: Bool = false

    var studyLaunch: StudyLaunchRequest?
    var captureRequest: CaptureSheetRequest?
    var editingDeckRequest: DeckEditRequest?
    var showNewDeckSheet: Bool = false
    var activeImportPicker: DeckImportKind?
    var deferredImportPicker: DeckImportKind?
    var pendingBackupImport: PendingBackupImport?
    var pendingApkgImport: PendingApkgImport?

    var isImporting: Bool {
        isImportingBackup || isImportingApkg
    }

    var canRequestDeckImport: Bool {
        !isImporting
            && activeImportPicker == nil
            && deferredImportPicker == nil
            && pendingBackupImport == nil
            && pendingApkgImport == nil
            && studyLaunch == nil
            && captureRequest == nil
            && editingDeckRequest == nil
    }

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
    private var deferredImportTask: Task<Void, Never>?

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

    /// Whether iOS Settings should offer native StoreKit 2 subscription purchase.
    /// Hidden when already active (including Polar web subscribers or allowlisted accounts),
    /// past_due, or self-hosted to prevent double-billing.
    var canShowAppleSubscribe: Bool {
        guard syncStatus?.signedIn == true else { return false }
        switch effectiveEntitlementState {
        case .none, .trialing, .expired:
            return true
        case .active, .pastDue, .selfHosted:
            return false
        }
    }

    /// Whether iOS Settings should offer Apple's native Manage Subscription sheet.
    var canShowAppleManageSubscription: Bool {
        guard syncStatus?.signedIn == true else { return false }
        guard entitlement?.provider == "apple" else { return false }
        return effectiveEntitlementState == .active || effectiveEntitlementState == .pastDue
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

    // MARK: - StoreKit 2 In-App Purchase

    func startStoreKitListener() {
        storeKit.startTransactionListener { [weak self] jws in
            guard let self else { return }
            guard self.syncStatus?.signedIn == true else {
                throw RemFfiError.NotAuthenticated(message: "Sign in to link your App Store subscription.")
            }
            try await self.verifyAppleTransactionJWS(jws, silent: true)
        }
    }

    func verifyAppleTransactionJWS(_ jws: String, silent: Bool = false) async throws {
        let profile = try await store.verifyAppleTransaction(signedTransactionInfo: jws)
        applyAccountProfile(profile)
        lastSyncError = nil
        if !isSyncPaused {
            await syncNow(silent: true)
        }
        if !silent {
            showToast("rem Premium active · syncing")
        }
    }

    func purchasePremiumViaApple() async -> Bool {
        guard let accountId = syncStatus?.accountId else {
            showToast("Sign in first to subscribe to rem Premium")
            return false
        }
        let outcome = await storeKit.purchase(accountId: accountId) { [weak self] jws in
            guard let self else { return }
            try await self.verifyAppleTransactionJWS(jws, silent: true)
        }
        switch outcome {
        case .purchased:
            showToast("rem Premium active · syncing")
            return true
        case .pending:
            showToast("Purchase pending approval")
            return false
        case .cancelled:
            return false
        case .failed(let message):
            showToast(message)
            return false
        }
    }

    func restoreApplePurchases() async -> Bool {
        guard syncStatus?.signedIn == true else {
            showToast("Sign in first to restore purchases")
            return false
        }
        let outcome = await storeKit.restorePurchases { [weak self] jws in
            guard let self else { return }
            try await self.verifyAppleTransactionJWS(jws, silent: true)
        }
        switch outcome {
        case .restored:
            showToast("App Store purchase restored · syncing")
            return true
        case .noActiveSubscription:
            showToast("No active App Store subscription found")
            return false
        case .failed(let message):
            showToast(message)
            return false
        }
    }

    func manageAppleSubscription() async {
        await storeKit.showManageSubscriptions()
        await refreshAccount(triggerSyncIfUnlocked: isSyncPaused)
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
        case .active: return "active"
        case .pastDue: return "past_due"
        case .expired: return "expired"
        }
    }

    private static func decodeState(_ raw: String) -> EntitlementStateDto? {
        switch raw {
        case "self_hosted": return .selfHosted
        case "none": return EntitlementStateDto.none
        case "trialing": return .trialing
        case "active": return .active
        case "past_due": return .pastDue
        case "expired": return .expired
        default: return nil
        }
    }

    // MARK: - Cloud OAuth & Account Lifecycle

    private func resolveTargetServerUrl(customServerUrl: String?) async -> String {
        if let custom = customServerUrl?.trimmingCharacters(in: .whitespacesAndNewlines), !custom.isEmpty {
            return custom
        }
        return await store.defaultCloudServerUrl()
    }

    func signIn(provider: OAuthProviderDto, customServerUrl: String? = nil) async {
        guard !isAuthenticating else { return }
        isAuthenticating = true
        defer { isAuthenticating = false }

        let targetUrl = await resolveTargetServerUrl(customServerUrl: customServerUrl)

        do {
            if provider == .apple {
                let credential = try await authManager.authenticateWithAppleNative()
                try await completeAppleNativeAuth(
                    serverUrl: targetUrl,
                    credential: credential,
                    resetExistingBinding: false
                )
                return
            }

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
            // Prompt user to re-authenticate with reset.
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

    func signInWithAppleNative(credential: NativeAppleCredential, customServerUrl: String? = nil) async {
        guard !isAuthenticating else { return }
        isAuthenticating = true
        defer { isAuthenticating = false }

        let targetUrl = await resolveTargetServerUrl(customServerUrl: customServerUrl)

        do {
            try await completeAppleNativeAuth(
                serverUrl: targetUrl,
                credential: credential,
                resetExistingBinding: false
            )
        } catch let RemFfiError.AccountMismatch(boundServerUrl, boundAccountId, _, incomingAccountId) {
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
            if provider == .apple {
                let credential = try await authManager.authenticateWithAppleNative()
                try await completeAppleNativeAuth(
                    serverUrl: serverUrl,
                    credential: credential,
                    resetExistingBinding: true
                )
                return
            }

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

    private func completeAppleNativeAuth(
        serverUrl: String,
        credential: NativeAppleCredential,
        resetExistingBinding: Bool
    ) async throws {
        _ = try await store.finishAppleNativeAuth(
            serverUrl: serverUrl,
            identityToken: credential.identityToken,
            authorizationCode: credential.authorizationCode,
            nonce: credential.rawNonce,
            email: credential.email,
            resetExistingBinding: resetExistingBinding
        )
        await finishPostSignIn()
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
        await finishPostSignIn()
    }

    private func finishPostSignIn() async {
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

    func requestDeckImport(_ kind: DeckImportKind, afterDelayMs: UInt64 = 0) {
        guard canRequestDeckImport else { return }
        if showNewDeckSheet {
            deferredImportPicker = kind
            showNewDeckSheet = false
        } else if afterDelayMs > 0 {
            deferredImportPicker = kind
            presentDeferredImportIfNeeded(delayMs: afterDelayMs)
        } else {
            activeImportPicker = kind
        }
    }

    func presentDeferredImportIfNeeded(delayMs: UInt64 = 150) {
        guard let kind = deferredImportPicker else { return }
        deferredImportTask?.cancel()
        if delayMs == 0 {
            deferredImportPicker = nil
            guard !isImporting,
                  activeImportPicker == nil,
                  pendingBackupImport == nil,
                  pendingApkgImport == nil,
                  !showNewDeckSheet,
                  studyLaunch == nil,
                  captureRequest == nil,
                  editingDeckRequest == nil else {
                return
            }
            activeImportPicker = kind
            return
        }
        deferredImportTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(delayMs))
            guard let self, !Task.isCancelled else { return }
            self.deferredImportPicker = nil
            guard !self.isImporting,
                  self.activeImportPicker == nil,
                  self.pendingBackupImport == nil,
                  self.pendingApkgImport == nil,
                  !self.showNewDeckSheet,
                  self.studyLaunch == nil,
                  self.captureRequest == nil,
                  self.editingDeckRequest == nil else {
                return
            }
            self.activeImportPicker = kind
        }
    }

    func handleBackupFileImportResult(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            guard let url = urls.first else { return }
            Task {
                if let pending = await prepareBackupImport(from: url) {
                    if pending.preview.replaced.isEmpty {
                        await commitBackupImport(json: pending.json)
                    } else {
                        pendingBackupImport = pending
                    }
                }
            }
        case .failure(let error):
            showToast("Could not open file: \(error.remDisplayMessage)")
        }
    }

    func handleApkgFileImportResult(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            guard let url = urls.first else { return }
            Task {
                if let pending = await prepareApkgImport(from: url) {
                    discardPendingApkgImport(pendingApkgImport)
                    pendingApkgImport = pending
                }
            }
        case .failure(let error):
            showToast("Could not open file: \(error.remDisplayMessage)")
        }
    }

    func cancelPendingBackupImport() {
        pendingBackupImport = nil
    }

    func confirmPendingBackupImport(_ pending: PendingBackupImport) {
        pendingBackupImport = nil
        Task { await commitBackupImport(json: pending.json) }
    }

    func cancelPendingApkgImport(_ pending: PendingApkgImport? = nil) {
        let target = pending ?? pendingApkgImport
        pendingApkgImport = nil
        discardPendingApkgImport(target)
    }

    func confirmPendingApkgImport(_ pending: PendingApkgImport) {
        if pendingApkgImport?.tempFileURL == pending.tempFileURL {
            pendingApkgImport = nil
        }
        Task { await commitApkgImport(tempFileURL: pending.tempFileURL) }
    }

    static func apkgImportAlertMessage(_ preview: ApkgImportPreviewDto) -> String {
        let deckCount = preview.added.count + preview.replaced.count
        let deckWord = deckCount == 1 ? "deck" : "decks"
        let cardWord = preview.totalCards == 1 ? "card" : "cards"
        let mediaPart = preview.mediaCount > 0 ? ", \(preview.mediaCount) media" : ""
        let revPart = preview.reviewLogs > 0 ? ", \(preview.reviewLogs) review logs" : ""
        var msg: String
        if preview.replaced.isEmpty {
            msg = "Found \(deckCount) \(deckWord), \(preview.totalCards) \(cardWord)\(mediaPart)\(revPart)."
        } else {
            let names = preview.replaced.joined(separator: ", ")
            msg = "Found \(deckCount) \(deckWord), \(preview.totalCards) \(cardWord)\(mediaPart)\(revPart). Existing deck(s) will be replaced: \(names)."
        }
        if !preview.warnings.isEmpty {
            let shown = preview.warnings.prefix(2).joined(separator: " ")
            msg += "\n\nNote: \(shown)"
        }
        return msg
    }

    func prepareBackupImport(from url: URL) async -> PendingBackupImport? {
        guard !isImporting else { return nil }
        isImportingBackup = true
        defer { isImportingBackup = false }
        do {
            let json = try await Task.detached(priority: .userInitiated) {
                let accessed = url.startAccessingSecurityScopedResource()
                defer { if accessed { url.stopAccessingSecurityScopedResource() } }
                return try String(contentsOf: url, encoding: .utf8)
            }.value
            let preview = try await store.previewBackupJson(json: json)
            if preview.added.isEmpty && preview.replaced.isEmpty {
                showToast("Backup file contains no decks.")
                return nil
            }
            return PendingBackupImport(json: json, preview: preview)
        } catch {
            showToast("Import failed: \(error.remDisplayMessage)")
            return nil
        }
    }

    func commitBackupImport(json: String) async {
        guard !isImporting else { return }
        isImportingBackup = true
        defer { isImportingBackup = false }
        do {
            let result = try await store.importBackupJson(json: json)
            await refreshAll()
            let total = result.added.count + result.replaced.count
            showToast("Imported \(total) \(total == 1 ? "deck" : "decks")")
            Task { await syncNow(silent: true) }
        } catch {
            showToast("Import failed: \(error.remDisplayMessage)")
        }
    }

    private static let maxIosApkgBytes: Int64 = 250 * 1024 * 1024

    func discardPendingApkgImport(_ pending: PendingApkgImport?) {
        guard let url = pending?.tempFileURL else { return }
        try? FileManager.default.removeItem(at: url)
    }

    func prepareApkgImport(from url: URL) async -> PendingApkgImport? {
        guard !isImporting else { return nil }
        isImportingApkg = true
        defer { isImportingApkg = false }
        var stagedURL: URL?
        do {
            let maxBytes = Self.maxIosApkgBytes
            let tempURL = try await Task.detached(priority: .userInitiated) { () throws -> URL in
                let accessed = url.startAccessingSecurityScopedResource()
                defer { if accessed { url.stopAccessingSecurityScopedResource() } }

                var coordError: NSError?
                var innerResult: Result<URL, Error>?
                let coordinator = NSFileCoordinator()
                coordinator.coordinate(readingItemAt: url, options: [], error: &coordError) { readableURL in
                    innerResult = Result {
                        let destURL = FileManager.default.temporaryDirectory
                            .appendingPathComponent("rem-import-\(UUID().uuidString).apkg")
                        guard let input = InputStream(url: readableURL) else {
                            throw RemFfiError.InvalidInput(message: "Could not open the selected .apkg file.")
                        }
                        FileManager.default.createFile(atPath: destURL.path, contents: nil)
                        let outHandle = try FileHandle(forWritingTo: destURL)
                        defer {
                            input.close()
                            try? outHandle.close()
                        }
                        input.open()
                        let chunkSize = 1024 * 1024
                        var buffer = [UInt8](repeating: 0, count: chunkSize)
                        var totalBytes: Int64 = 0
                        while true {
                            let readCount = input.read(&buffer, maxLength: chunkSize)
                            if readCount < 0 {
                                try? FileManager.default.removeItem(at: destURL)
                                throw input.streamError ?? RemFfiError.InvalidInput(message: "Failed reading .apkg archive.")
                            }
                            if readCount == 0 { break }
                            totalBytes += Int64(readCount)
                            if totalBytes > maxBytes {
                                try? FileManager.default.removeItem(at: destURL)
                                throw RemFfiError.InvalidInput(message: "Anki archive exceeds the 250 MB iOS import limit.")
                            }
                            try outHandle.write(contentsOf: buffer[0..<readCount])
                        }
                        return destURL
                    }
                }
                if let coordError {
                    throw coordError
                }
                guard let innerResult else {
                    throw RemFfiError.InvalidInput(message: "Could not read the selected .apkg file.")
                }
                return try innerResult.get()
            }.value
            stagedURL = tempURL
            let preview = try await store.previewApkgPath(path: tempURL.path)
            if preview.added.isEmpty && preview.replaced.isEmpty {
                try? FileManager.default.removeItem(at: tempURL)
                showToast("No importable cards found in this .apkg file.")
                return nil
            }
            return PendingApkgImport(tempFileURL: tempURL, preview: preview)
        } catch {
            if let stagedURL {
                try? FileManager.default.removeItem(at: stagedURL)
            }
            showToast("Anki import failed: \(error.remDisplayMessage)")
            return nil
        }
    }

    func commitApkgImport(tempFileURL: URL) async {
        guard !isImporting else {
            try? FileManager.default.removeItem(at: tempFileURL)
            return
        }
        isImportingApkg = true
        defer {
            isImportingApkg = false
            try? FileManager.default.removeItem(at: tempFileURL)
        }
        do {
            let result = try await store.importApkgPath(path: tempFileURL.path)
            await refreshAll()
            let decksCount = result.added.count + result.replaced.count
            let warnSuffix = result.warnings.isEmpty ? "" : " (\(result.warnings.count) warning\(result.warnings.count == 1 ? "" : "s"))"
            showToast("Imported \(result.importedCards) cards across \(decksCount) \(decksCount == 1 ? "deck" : "decks")\(warnSuffix)")
            Task { await syncNow(silent: true) }
        } catch {
            showToast("Anki import failed: \(error.remDisplayMessage)")
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
            showToast(error.remDisplayMessage)
            return nil
        }
    }

    func updateDeck(
        id: String,
        name: String?,
        color: String?,
        settings: DeckSettingsPatchDto? = nil
    ) async -> DeckDto? {
        do {
            let updated = try await store.updateDeck(
                id: id,
                name: name,
                color: color,
                settings: settings
            )
            await refreshAll()
            showToast("Deck updated")
            Task { await syncNow(silent: true) }
            return updated
        } catch {
            showToast(error.remDisplayMessage)
            return nil
        }
    }

    func optimizeDeckFsrs(id: String) async -> DeckDto? {
        do {
            let updated = try await store.optimizeDeckFsrs(deckId: id)
            await refreshAll()
            showToast("Personalized schedule updated")
            Task { await syncNow(silent: true) }
            return updated
        } catch {
            showToast(error.remDisplayMessage)
            return nil
        }
    }

    func resetDeckFsrs(id: String) async -> DeckDto? {
        do {
            let updated = try await store.resetDeckFsrs(deckId: id)
            await refreshAll()
            showToast("Reset to default schedule")
            Task { await syncNow(silent: true) }
            return updated
        } catch {
            showToast(error.remDisplayMessage)
            return nil
        }
    }

    func openEditDeck(deckId: String, onDeleted: (() -> Void)? = nil) {
        editingDeckRequest = DeckEditRequest(deckId: deckId, onDeleted: onDeleted)
    }

    func deleteDeck(id: String) async {
        let deckName = decks.first(where: { $0.id == id })?.name
            ?? todaySummary?.decks.first(where: { $0.id == id })?.name
        do {
            try await store.deleteDeck(id: id)
            _ = try? await store.sweepOrphanAssets()
            if let deckName {
                pruneCollapsedDeckPaths(removing: [deckName])
            }
            if activeDeckId == id {
                activeDeckId = nil
            }
            await refreshAll()
            showToast("Deck deleted")
            Task { await syncNow(silent: true) }
        } catch {
            showToast(error.remDisplayMessage)
        }
    }

    func deleteDeckTree(path: String) async {
        do {
            let deletedIds = try await store.deleteDeckTree(path: path)
            _ = try? await store.sweepOrphanAssets()
            pruneCollapsedDeckPaths(removing: [path])
            if let activeDeckId, deletedIds.contains(activeDeckId) {
                self.activeDeckId = nil
            }
            await refreshAll()
            showToast(deletedIds.count == 1 ? "Deck deleted" : "Deleted \(deletedIds.count) decks")
            Task { await syncNow(silent: true) }
        } catch {
            showToast(error.remDisplayMessage)
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

    func startCustomStudy(deckId: String, mode: CustomStudyModeDto, amount: UInt32) {
        studyLaunch = StudyLaunchRequest(
            deckId: deckId,
            customStudy: CustomStudySpec(mode: mode, amount: amount)
        )
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
