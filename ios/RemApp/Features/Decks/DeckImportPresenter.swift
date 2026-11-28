import SwiftUI
import UniformTypeIdentifiers

struct DeckImportPresenterModifier: ViewModifier {
    @Bindable var appState: AppState

    private var apkgImportContentTypes: [UTType] {
        var types: [UTType] = [.zip, .data]
        if let colpkg = UTType(filenameExtension: "colpkg") {
            types.insert(colpkg, at: 0)
        }
        if let apkg = UTType(filenameExtension: "apkg") {
            types.insert(apkg, at: 0)
        }
        return types
    }

    func body(content: Content) -> some View {
        content
            .background {
                Color.clear
                    .fileImporter(
                        isPresented: Binding(
                            get: { appState.activeImportPicker == .backup },
                            set: { if !$0 && appState.activeImportPicker == .backup { appState.activeImportPicker = nil } }
                        ),
                        allowedContentTypes: [.json],
                        allowsMultipleSelection: false
                    ) { result in
                        appState.handleBackupFileImportResult(result)
                    }
            }
            .background {
                Color.clear
                    .fileImporter(
                        isPresented: Binding(
                            get: { appState.activeImportPicker == .apkg },
                            set: { if !$0 && appState.activeImportPicker == .apkg { appState.activeImportPicker = nil } }
                        ),
                        allowedContentTypes: apkgImportContentTypes,
                        allowsMultipleSelection: false
                    ) { result in
                        appState.handleApkgFileImportResult(result)
                    }
            }
            .alert(
                "Import Anki deck?",
                isPresented: Binding(
                    get: { appState.pendingApkgImport != nil },
                    set: { _ in }
                ),
                presenting: appState.pendingApkgImport
            ) { pending in
                Button(
                    pending.preview.replaced.isEmpty ? "Import" : "Import & replace",
                    role: pending.preview.replaced.isEmpty ? nil : .destructive
                ) {
                    appState.confirmPendingApkgImport(pending)
                }
                Button("Cancel", role: .cancel) {
                    appState.cancelPendingApkgImport(pending)
                }
            } message: { pending in
                Text(AppState.apkgImportAlertMessage(pending.preview))
            }
            .alert(
                "Replace existing decks?",
                isPresented: Binding(
                    get: { appState.pendingBackupImport != nil },
                    set: { _ in }
                ),
                presenting: appState.pendingBackupImport
            ) { pending in
                Button("Import & replace", role: .destructive) {
                    appState.confirmPendingBackupImport(pending)
                }
                Button("Cancel", role: .cancel) {
                    appState.cancelPendingBackupImport()
                }
            } message: { pending in
                let names = pending.preview.replaced.joined(separator: ", ")
                if pending.preview.added.isEmpty {
                    Text("Importing will replace existing deck(s): \(names).")
                } else {
                    Text("Importing will replace existing deck(s): \(names). New decks (\(pending.preview.added.count)) will be added alongside them.")
                }
            }
    }
}

extension View {
    func deckImportPresenter(appState: AppState) -> some View {
        modifier(DeckImportPresenterModifier(appState: appState))
    }
}
