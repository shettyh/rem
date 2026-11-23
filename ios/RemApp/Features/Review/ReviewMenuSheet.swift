import SwiftUI

/// Compact native inspector sheet showing FSRS scheduling details for the current card.
struct ReviewCardInfoSheet: View {
    let card: CardDto
    let deckName: String?

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section("Schedule") {
                    LabeledContent("Status") {
                        RemPillBadge(text: stateLabel, tone: stateTone)
                    }
                    LabeledContent("Reviews") {
                        Text("\(card.reps)")
                            .font(.body.monospacedDigit())
                            .foregroundStyle(RemColor.text)
                    }
                    LabeledContent("Lapses") {
                        Text("\(card.lapses)")
                            .font(.body.monospacedDigit())
                            .foregroundStyle(card.lapses > 0 ? RemColor.hard : RemColor.text)
                    }
                    LabeledContent("Due") {
                        Text(dueText)
                            .foregroundStyle(RemColor.muted)
                    }
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)

                Section("Details") {
                    if let deckName, !deckName.isEmpty {
                        LabeledContent("Deck", value: deckName)
                    }
                    LabeledContent("Created", value: formattedDate(card.createdAt))
                    if !card.tags.isEmpty {
                        LabeledContent("Tags") {
                            Text(card.tags.map { "#\($0)" }.joined(separator: " "))
                                .font(.footnote)
                                .foregroundStyle(RemColor.muted)
                                .multilineTextAlignment(.trailing)
                        }
                    }
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)
            }
            .scrollContentBackground(.hidden)
            .background(RemColor.groupedBg)
            .navigationTitle("Card Info")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .fontWeight(.semibold)
                }
            }
        }
        .tint(RemColor.accent)
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
    }

    private var stateLabel: String {
        if card.suspended { return "Suspended" }
        switch card.state {
        case 0: return "New"
        case 1: return "Learning"
        case 2: return "Review"
        case 3: return "Relearning"
        default: return "Card"
        }
    }

    private var stateTone: RemPillBadge.Tone {
        if card.suspended { return .neutral }
        switch card.state {
        case 0: return .accent
        case 1, 3: return .hard
        case 2: return .good
        default: return .neutral
        }
    }

    private var dueText: String {
        let nowMs = RemStore.nowMs()
        if card.state == 0 { return "New card" }
        if card.due <= nowMs { return "Due now" }
        let date = Date(timeIntervalSince1970: TimeInterval(card.due) / 1000.0)
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    private func formattedDate(_ epochMs: Int64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(epochMs) / 1000.0)
        return date.formatted(date: .abbreviated, time: .omitted)
    }
}
