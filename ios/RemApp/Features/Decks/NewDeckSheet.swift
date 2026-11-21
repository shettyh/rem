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
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("DECK NAME")
                        .font(.system(size: 11, weight: .bold))
                        .tracking(0.8)
                        .foregroundStyle(RemColor.muted)

                    TextField("e.g. Distributed systems, Rust…", text: $name)
                        .font(.system(size: 16))
                        .padding(13)
                        .background(RemColor.surface)
                        .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 11, style: .continuous)
                                .strokeBorder(RemColor.borderStrong, lineWidth: 1)
                        )
                }

                VStack(alignment: .leading, spacing: 10) {
                    Text("COLOR")
                        .font(.system(size: 11, weight: .bold))
                        .tracking(0.8)
                        .foregroundStyle(RemColor.muted)

                    HStack(spacing: 14) {
                        ForEach(RemColor.deckPalette, id: \.self) { hex in
                            Button {
                                selectedColor = hex
                            } label: {
                                Circle()
                                    .fill(Color(hexString: hex))
                                    .frame(width: 30, height: 30)
                                    .overlay(
                                        Circle()
                                            .strokeBorder(RemColor.text, lineWidth: selectedColor == hex ? 2.5 : 0)
                                            .padding(-4)
                                    )
                            }
                            .accessibilityLabel("Color \(hex)")
                        }
                    }
                }

                Spacer()
            }
            .padding(20)
            .background(RemColor.bg)
            .navigationTitle("New deck")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundStyle(RemColor.muted)
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
                    .foregroundStyle(canSave ? RemColor.accent : RemColor.faint)
                    .disabled(!canSave)
                }
            }
        }
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
    }
}
