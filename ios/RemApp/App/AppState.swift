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

    var selectedTab: RemTab = .today
    var activeDeckId: String?

    var todaySummary: TodaySummaryDto?
    var decks: [DeckDto] = []
    var syncStatus: SyncStatusDto?
    var storageSummary: StorageSummaryDto?

    var isLoading: Bool = true
    var isSyncing: Bool = false
    var lastSyncedAt: Date?
    var lastSyncError: String?
    var toastMessage: String?

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

    private enum Keys {
        static let dismissedOnboarding = "rem.ios.dismissedOnboarding"
        static let cardTextSize = "rem.ios.cardTextSize"
        static let appearance = "rem.ios.appearance"
        static let hapticsEnabled = "rem.ios.hapticsEnabled"
        static let reminderEnabled = "rem.ios.reminderEnabled"
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
            if !fetchedDecks.isEmpty {
                self.hasDismissedOnboarding = true
            }
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
            try? await Task.sleep(for: .seconds(2.4))
            guard !Task.isCancelled else { return }
            toastMessage = nil
        }
    }
}
