import SwiftUI

private extension CustomStudyModeDto {
    var defaultAmount: Int {
        switch self {
        case .studyAhead, .reviewForgotten:
            return 2
        case .increaseNew, .previewNew:
            return 10
        }
    }

    func amountUnit(for amount: Int) -> String {
        switch self {
        case .studyAhead, .reviewForgotten:
            return amount == 1 ? "day" : "days"
        case .increaseNew, .previewNew:
            return amount == 1 ? "card" : "cards"
        }
    }

    func description(for amount: Int) -> String {
        let unit = amountUnit(for: amount)
        switch self {
        case .studyAhead:
            return "Review cards scheduled in the next \(amount) \(unit)."
        case .increaseNew:
            return "Introduce up to \(amount) extra new \(unit) beyond today's daily limit."
        case .reviewForgotten:
            return "Review cards graded Again within the last \(amount) \(unit)."
        case .previewNew:
            return "Preview up to \(amount) new \(unit) without recording reviews or changing schedules."
        }
    }
}

struct CustomStudySheet: View {
    @Bindable var appState: AppState
    let deckId: String
    let deckName: String

    @Environment(\.dismiss) private var dismiss
    @State private var mode: CustomStudyModeDto = .studyAhead
    @State private var amount: Int = 1

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Custom study mode", selection: $mode) {
                        Text("Study ahead").tag(CustomStudyModeDto.studyAhead)
                        Text("Increase new limit").tag(CustomStudyModeDto.increaseNew)
                        Text("Review forgotten").tag(CustomStudyModeDto.reviewForgotten)
                        Text("Preview new cards").tag(CustomStudyModeDto.previewNew)
                    }
                    .pickerStyle(.inline)
                    .labelsHidden()
                    .onChange(of: mode) { _, next in
                        amount = next.defaultAmount
                    }
                } header: {
                    Text("Mode · \(deckName)")
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)

                Section {
                    Stepper(value: $amount, in: 1...999) {
                        HStack {
                            Text("Amount")
                                .foregroundStyle(RemColor.text)
                            Spacer()
                            Text("\(amount) \(mode.amountUnit(for: amount))")
                                .font(.body.monospacedDigit())
                                .foregroundStyle(RemColor.muted)
                        }
                    }
                } footer: {
                    Text(mode.description(for: amount))
                        .font(.caption)
                        .foregroundStyle(RemColor.muted)
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)

                Section {
                    Button {
                        dismiss()
                        appState.startCustomStudy(
                            deckId: deckId,
                            mode: mode,
                            amount: UInt32(amount)
                        )
                    } label: {
                        Text("Start custom study")
                            .fontWeight(.semibold)
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .tint(RemColor.accent)
                    .listRowInsets(EdgeInsets(top: 6, leading: 0, bottom: 6, trailing: 0))
                    .listRowBackground(Color.clear)
                }
            }
            .scrollContentBackground(.hidden)
            .background(RemColor.groupedBg)
            .navigationTitle("Custom Study")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
        .tint(RemColor.accent)
    }
}
