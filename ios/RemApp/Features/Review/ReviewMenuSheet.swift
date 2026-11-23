import SwiftUI

struct ReviewMenuSheet: View {
    @Bindable var appState: AppState
    let canUndo: Bool
    let hasCurrentCard: Bool
    let onEdit: () -> Void
    let onUndo: () -> Void
    let onSuspend: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    if hasCurrentCard {
                        Button {
                            dismiss()
                            onEdit()
                        } label: {
                            Label("Edit card", systemImage: "pencil")
                                .foregroundStyle(RemColor.text)
                        }
                    }

                    Button {
                        dismiss()
                        onUndo()
                    } label: {
                        HStack {
                            Label("Undo last grade", systemImage: "arrow.uturn.backward")
                                .foregroundStyle(canUndo ? RemColor.text : RemColor.muted)
                            Spacer()
                            if !canUndo {
                                Text("None yet")
                                    .font(.footnote)
                                    .foregroundStyle(RemColor.faint)
                            }
                        }
                    }
                    .disabled(!canUndo)

                    HStack {
                        Label("Text size", systemImage: "textformat.size")
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        Picker("Text size", selection: $appState.cardTextSize) {
                            ForEach(CardTextSize.allCases) { size in
                                Text(size.shortLabel).tag(size)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 124)
                    }
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)

                if hasCurrentCard {
                    Section {
                        Button(role: .destructive) {
                            dismiss()
                            onSuspend()
                        } label: {
                            Label("Suspend card", systemImage: "pause.circle")
                        }
                    }
                    .listRowBackground(RemColor.surface)
                }
            }
            .scrollContentBackground(.hidden)
            .background(RemColor.groupedBg)
            .navigationTitle("Card & Session")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .fontWeight(.semibold)
                }
            }
        }
        .tint(RemColor.accent)
        .presentationDetents([.height(340)])
        .presentationDragIndicator(.visible)
    }
}
