import SwiftUI

struct NewDeckSheet: View {
    @Bindable var appState: AppState
    @Environment(\.dismiss) private var dismiss

    @State private var name: String = ""
    @State private var selectedColor: String = RemColor.deckPalette[0]
    @State private var isSaving: Bool = false

    private var canSave: Bool {
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !isSaving
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("e.g. Spanish::Verbs, Rust…", text: $name)
                        .foregroundStyle(RemColor.text)
                } header: {
                    Text("Deck Name")
                } footer: {
                    Text("Use :: to create nested subdecks (e.g. Medicine::Cardiology).")
                }
                .listRowBackground(RemColor.surface)

                Section("Color") {
                    HStack(spacing: 14) {
                        ForEach(RemColor.deckPalette, id: \.self) { hex in
                            let isSelected = selectedColor == hex
                            Button {
                                selectedColor = hex
                            } label: {
                                Circle()
                                    .fill(Color(hexString: hex))
                                    .frame(width: 30, height: 30)
                                    .overlay(
                                        Circle()
                                            .strokeBorder(RemColor.text, lineWidth: isSelected ? 2.5 : 0)
                                            .padding(-4)
                                    )
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Color \(hex)")
                            .accessibilityAddTraits(isSelected ? .isSelected : [])
                        }
                    }
                    .padding(.vertical, 6)
                }
                .listRowBackground(RemColor.surface)

                Section {
                    Button {
                        appState.requestDeckImport(.apkg)
                    } label: {
                        HStack {
                            Label("Import Anki deck (.apkg)", systemImage: "tray.and.arrow.down")
                                .foregroundStyle(RemColor.text)
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(RemColor.faint)
                        }
                    }
                    .disabled(isSaving || !appState.canRequestDeckImport)

                    Button {
                        appState.requestDeckImport(.backup)
                    } label: {
                        HStack {
                            Label("Import backup (JSON)", systemImage: "square.and.arrow.down")
                                .foregroundStyle(RemColor.text)
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(RemColor.faint)
                        }
                    }
                    .disabled(isSaving || !appState.canRequestDeckImport)
                } header: {
                    Text("Or Import")
                } footer: {
                    Text("Import decks, cloze cards, LaTeX, media, and review history from Anki, or restore a rem JSON backup.")
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)
            }
            .scrollContentBackground(.hidden)
            .background(RemColor.groupedBg)
            .navigationTitle("New deck")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        isSaving = true
                        Task {
                            let created = await appState.createDeck(
                                name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                                color: selectedColor
                            )
                            isSaving = false
                            if created != nil {
                                dismiss()
                            }
                        }
                    }
                    .fontWeight(.semibold)
                    .disabled(!canSave)
                }
            }
        }
        .tint(RemColor.accent)
        .remToastOverlay(appState: appState)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }
}
