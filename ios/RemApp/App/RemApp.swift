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
            Task {
                await state.refreshAll()
                await state.syncNow(silent: true)
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
        ZStack(alignment: .top) {
            if appState.shouldShowFirstRun {
                FirstRunView(appState: appState)
            } else {
                RootTabView(appState: appState)
            }

            if let toast = appState.toastMessage {
                HStack(spacing: 10) {
                    Text(toast)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(RemColor.bg)
                        .lineLimit(2)
                    Spacer(minLength: 4)
                    Button("OK") {
                        appState.toastMessage = nil
                    }
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(RemColor.bg.opacity(0.75))
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 11)
                .background(RemColor.text)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                .shadow(color: Color.black.opacity(0.18), radius: 14, y: 6)
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .animation(.easeOut(duration: 0.18), value: appState.toastMessage)
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                Task {
                    await appState.refreshAll()
                    await appState.syncNow(silent: true)
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
    }
}
