import SwiftUI

@main
struct RemApp: App {
    @State private var appState: AppState?
    @State private var bootError: String?

    var body: some Scene {
        WindowGroup {
            Group {
                if let appState {
                    RootContainerView(appState: appState)
                        .preferredColorScheme(appState.appearance.colorScheme)
                } else if let bootError {
                    VStack(spacing: 12) {
                        RemLogoBadge(size: 44)
                        Text("Could not open local collection")
                            .font(.headline)
                            .foregroundStyle(RemColor.text)
                        Text(bootError)
                            .font(.footnote)
                            .foregroundStyle(RemColor.muted)
                            .multilineTextAlignment(.center)
                    }
                    .padding(28)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(RemColor.bg)
                } else {
                    RemColor.bg
                        .ignoresSafeArea()
                        .task { bootstrap() }
                }
            }
        }
    }

    @MainActor
    private func bootstrap() {
        guard appState == nil else { return }
        do {
            let dbPath = try RemStore.defaultDatabasePath()
            let store = try RemStore(dbPath: dbPath)
            let state = AppState(store: store)
            self.appState = state
            if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil {
                state.startStoreKitListener()
            }
            Task {
                await state.refreshAll()
                if state.syncStatus?.signedIn == true {
                    await state.refreshAccount(triggerSyncIfUnlocked: false)
                    if !state.isSyncPaused {
                        await state.syncNow(silent: true)
                    }
                }
            }
        } catch {
            self.bootError = error.localizedDescription
        }
    }
}

struct RootContainerView: View {
    @Bindable var appState: AppState
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        Group {
            if appState.shouldShowFirstRun {
                FirstRunView(appState: appState)
            } else {
                RootTabView(appState: appState)
            }
        }
        .remToastOverlay(
            appState: appState,
            enabled: appState.studyLaunch == nil && appState.captureRequest == nil
        )
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                Task {
                    await appState.refreshAll()
                    if appState.isSyncPaused {
                        await appState.refreshAccount(triggerSyncIfUnlocked: true)
                    } else {
                        await appState.syncNow(silent: true)
                    }
                }
            }
        }
        .fullScreenCover(item: $appState.studyLaunch, onDismiss: {
            Task {
                await appState.refreshAll()
                await appState.syncNow(silent: true)
            }
        }) { request in
            ReviewScreen(appState: appState, deckId: request.deckId)
        }
        .sheet(item: $appState.captureRequest) { request in
            QuickCaptureSheet(
                appState: appState,
                initialDeckId: request.deckId,
                editingCard: request.editingCard,
                onSaved: request.onSaved
            )
        }
        .sheet(isPresented: $appState.showNewDeckSheet) {
            NewDeckSheet(appState: appState)
        }
        .sheet(item: $appState.editingDeckRequest) { request in
            DeckOptionsSheet(
                appState: appState,
                deckId: request.deckId,
                onDeleted: request.onDeleted
            )
        }
    }
}
