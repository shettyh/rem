import SwiftUI

struct CustomStudySheet: View {
    @Bindable var appState: AppState
    let deckId: String
    let deckName: String

    @Environment(\.dismiss) private var dismiss
    @State private var mode: CustomStudyModeDto = .studyAhead
    @State private var amount: Int = 1

    private var amountUnit: String {
        switch mode {
        case .studyAhead, .reviewForgotten:
            return amount == 1 ? "day" : "days"
        case .increaseNew, .previewNew:
            return amount == 1 ? "card" : "cards"
        }
    }

    private var modeDescription: String {
        switch mode {
        case .studyAhead:
            return "Review cards scheduled in the next \(amount) \(amountUnit)."
        case .increaseNew:
            return "Introduce up to \(amount) extra new \(amountUnit) beyond today's daily limit."
        case .reviewForgotten:
            return "Review cards graded Again within the last \(amount) \(amountUnit)."
        case .previewNew:
            return "Preview up to \(amount) new \(amountUnit) without recording reviews or changing schedules."
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Mode") {
                    Picker("Custom study mode", selection: $mode) {
                        Text("Study ahead").tag(CustomStudyModeDto.studyAhead)
                        Text("Increase new limit").tag(CustomStudyModeDto.increaseNew)
                        Text("Review forgotten").tag(CustomStudyModeDto.reviewForgotten)
                        Text("Preview new cards").tag(CustomStudyModeDto.previewNew)
                    }
                    .pickerStyle(.inline)
                    .labelsHidden()
                    .onChange(of: mode) { _, next in
                        switch next {
                        case .studyAhead, .reviewForgotten:
                            amount = 2
                        case .increaseNew, .previewNew:
                            amount = 10
                        }
                    }
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)

                Section {
                    Stepper(value: $amount, in: 1...999) {
                        HStack {
                            Text("Amount")
                                .foregroundStyle(RemColor.text)
                            Spacer()
                            Text("\(amount) \(amountUnit)")
                                .font(.body.monospacedDigit())
                                .foregroundStyle(RemColor.muted)
                        }
                    }
                } footer: {
                    Text(modeDescription)
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
