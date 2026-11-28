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
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
    }
}
