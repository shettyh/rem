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
            VStack(spacing: 14) {
                VStack(spacing: 0) {
                    if hasCurrentCard {
                        menuRow(
                            title: "Edit card",
                            detail: "Markdown",
                            action: {
                                dismiss()
                                onEdit()
                            }
                        )
                        Divider().overlay(RemColor.border)
                    }

                    menuRow(
                        title: "Undo last grade",
                        detail: canUndo ? "Restore" : "None yet",
                        disabled: !canUndo,
                        action: {
                            dismiss()
                            onUndo()
                        }
                    )

                    Divider().overlay(RemColor.border)

                    HStack {
                        Text("Text size")
                            .font(.system(size: 15, weight: .medium))
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
                    .padding(.horizontal, 15)
                    .padding(.vertical, 12)

                    if hasCurrentCard {
                        Divider().overlay(RemColor.border)
                        menuRow(
                            title: "Suspend card",
                            detail: "Skip in queue",
                            destructive: true,
                            action: {
                                dismiss()
                                onSuspend()
                            }
                        )
                    }
                }
                .background(RemColor.surface)
                .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )

                Spacer()
            }
            .padding(16)
            .background(RemColor.bg)
            .navigationTitle("Card & Session")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .fontWeight(.semibold)
                        .foregroundStyle(RemColor.accent)
                }
            }
        }
        .presentationDetents([.height(310)])
        .presentationDragIndicator(.visible)
    }

    private func menuRow(
        title: String,
        detail: String,
        destructive: Bool = false,
        disabled: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack {
                Text(title)
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(destructive ? RemColor.danger : RemColor.text)
                Spacer()
                Text(detail)
                    .font(.system(size: 12.5))
                    .foregroundStyle(destructive ? RemColor.danger.opacity(0.8) : RemColor.muted)
            }
            .padding(.horizontal, 15)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
            .opacity(disabled ? 0.42 : 1.0)
        }
        .disabled(disabled)
        .buttonStyle(.plain)
    }
}
