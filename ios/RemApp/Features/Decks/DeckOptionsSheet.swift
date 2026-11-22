import SwiftUI

enum DeckStepParser {
    /// Split a space-separated steps string into chip tokens, e.g. `"1m 10m 1d"` → `["1m", "10m", "1d"]`.
    static func parseTokens(_ raw: String) -> [String] {
        raw.split(whereSeparator: { $0.isWhitespace }).map(String.init)
    }

    /// Mirrors `rem-core`'s step parser: `<digits>[s|m|h|d]` or bare `<digits>` (minutes).
    static func isValidToken(_ token: String) -> Bool {
        let trimmed = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let last = trimmed.last else { return false }
        let numberPart: Substring
        switch last {
        case "s", "m", "h", "d":
            numberPart = trimmed.dropLast()
        case let ch where ch.isASCII && ch.isNumber:
            numberPart = trimmed[...]
        default:
            return false
        }
        guard !numberPart.isEmpty else { return false }
        return UInt64(numberPart) != nil
    }
}

struct DeckOptionsSheet: View {
    @Bindable var appState: AppState
    let deckId: String
    let onDeleted: (() -> Void)?

    @Environment(\.dismiss) private var dismiss

    @State private var name: String = ""
    @State private var selectedColor: String = RemColor.deckPalette[0]
    @State private var initialColor: String = RemColor.deckPalette[0]

    @State private var desiredRetentionPercent: Int = 90
    @State private var newPerDay: Int = 20
    @State private var maxReviews: Int = 200
    @State private var learnSteps: String = "1m 10m"
    @State private var insertionOrder: InsertionOrderDto = .sequential
    @State private var relearnSteps: String = "10m"
    @State private var minimumInterval: Int = 1
    @State private var leechThreshold: Int = 8
    @State private var leechAction: LeechActionDto = .suspend
    @State private var buryRelated: Bool = true
    @State private var showTimer: Bool = false
    @State private var maximumInterval: UInt32 = 36_500

    @State private var fsrsStatus: DeckFsrsStatusDto?
    @State private var isLoaded: Bool = false
    @State private var isSaving: Bool = false
    @State private var isOptimizing: Bool = false
    @State private var showDeleteConfirm: Bool = false

    private var canSave: Bool {
        isLoaded
            && !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !isSaving
            && !isOptimizing
    }

    private var paletteOptions: [String] {
        let normalizedInitial = initialColor.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let existsInPalette = RemColor.deckPalette.contains { $0.lowercased() == normalizedInitial }
        if !normalizedInitial.isEmpty && !existsInPalette {
            return [initialColor] + RemColor.deckPalette
        }
        return RemColor.deckPalette
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    generalSection
                    dailyLimitsSection
                    newCardsSection
                    lapsesSection
                    dangerZoneSection
                }
                .padding(.horizontal, 18)
                .padding(.top, 14)
                .padding(.bottom, 32)
            }
            .scrollDismissesKeyboard(.interactively)
            .background(RemColor.bg)
            .navigationTitle("Deck options")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundStyle(RemColor.muted)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(isSaving ? "Saving…" : "Save") {
                        Task { await saveChanges() }
                    }
                    .fontWeight(.semibold)
                    .foregroundStyle(canSave ? RemColor.accent : RemColor.faint)
                    .disabled(!canSave)
                }
            }
        }
        .task {
            await loadDeck()
        }
        .alert("Delete deck?", isPresented: $showDeleteConfirm) {
            Button("Delete", role: .destructive) {
                Task {
                    await appState.deleteDeck(id: deckId)
                    dismiss()
                    onDeleted?()
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This deletes the deck and all its cards locally and on synced devices.")
        }
    }

    // MARK: - Sections

    private var generalSection: some View {
        section("GENERAL") {
            VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Deck name")
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundStyle(RemColor.muted)

                    TextField("Deck name", text: $name)
                        .font(.system(size: 16))
                        .foregroundStyle(RemColor.text)
                        .padding(11)
                        .background(RemColor.bgInset)
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .strokeBorder(RemColor.borderStrong, lineWidth: 1)
                        )
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                VStack(alignment: .leading, spacing: 10) {
                    Text("Color")
                        .font(.system(size: 12.5, weight: .semibold))
                        .foregroundStyle(RemColor.muted)

                    HStack(spacing: 14) {
                        ForEach(paletteOptions, id: \.self) { hex in
                            let isSelected = selectedColor.lowercased() == hex.lowercased()
                            Button {
                                selectedColor = hex
                            } label: {
                                Circle()
                                    .fill(Color(hexString: hex))
                                    .frame(width: 28, height: 28)
                                    .overlay(
                                        Circle()
                                            .strokeBorder(RemColor.text, lineWidth: isSelected ? 2.5 : 0)
                                            .padding(-4)
                                    )
                            }
                            .accessibilityLabel("Color \(hex)")
                            .accessibilityAddTraits(isSelected ? .isSelected : [])
                        }
                    }
                    .padding(.vertical, 2)
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Desired retention")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("Target recall probability. Higher values schedule more frequent reviews.")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer(minLength: 4)
                    RemStepperControl(
                        value: $desiredRetentionPercent,
                        range: 70...99,
                        step: 1,
                        label: "Desired retention",
                        formattedValue: "\(desiredRetentionPercent)%"
                    )
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                personalizedSchedulingRow
                    .padding(14)
            }
        }
    }

    private var personalizedSchedulingRow: some View {
        let hasWeights = fsrsStatus?.hasPersonalizedWeights ?? false
        let reviewCount = Int(fsrsStatus?.reviewCount ?? 0)
        let canOptimize = fsrsStatus?.canOptimize ?? false

        return VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Personalized scheduling")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(RemColor.text)
                    Text(hasWeights ? "Personalized schedule" : "Default schedule")
                        .font(.system(size: 12))
                        .foregroundStyle(RemColor.muted)
                    Text("\(reviewCount) recorded \(reviewCount == 1 ? "review" : "reviews")")
                        .font(.system(size: 12))
                        .foregroundStyle(RemColor.muted)
                    if !canOptimize {
                        Text("Optimize after reviewing a card on a later day.")
                            .font(.system(size: 11.5))
                            .foregroundStyle(RemColor.faint)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 4)
            }

            HStack(spacing: 10) {
                Button {
                    Task { await optimizeFsrs() }
                } label: {
                    HStack(spacing: 6) {
                        if isOptimizing {
                            ProgressView()
                                .controlSize(.small)
                                .tint(RemColor.onAccent)
                        }
                        Text(isOptimizing ? "Optimizing…" : "Optimize")
                            .font(.system(size: 13, weight: .semibold))
                    }
                    .foregroundStyle(canOptimize && !isOptimizing ? RemColor.onAccent : RemColor.muted)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(canOptimize && !isOptimizing ? RemColor.accent : RemColor.bgInset)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
                .disabled(!canOptimize || isOptimizing || isSaving)

                if hasWeights {
                    Button {
                        Task { await resetFsrs() }
                    } label: {
                        Text("Reset")
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(RemColor.text)
                            .padding(.horizontal, 14)
                            .padding(.vertical, 8)
                            .background(RemColor.surface)
                            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                            .overlay(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .strokeBorder(RemColor.borderStrong, lineWidth: 1)
                            )
                    }
                    .disabled(isOptimizing || isSaving)
                }
            }
        }
    }

    private var dailyLimitsSection: some View {
        section("DAILY LIMITS") {
            VStack(spacing: 0) {
                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("New cards/day")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("Cap on new cards introduced daily.")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer(minLength: 4)
                    RemStepperControl(
                        value: $newPerDay,
                        range: 0...9999,
                        step: 5,
                        label: "New cards per day",
                        formattedValue: "\(newPerDay)"
                    )
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Maximum reviews/day")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("Cap on due reviews shown each day.")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer(minLength: 4)
                    RemStepperControl(
                        value: $maxReviews,
                        range: 0...9999,
                        step: 10,
                        label: "Maximum reviews per day",
                        formattedValue: "\(maxReviews)"
                    )
                }
                .padding(14)
            }
        }
    }

    private var newCardsSection: some View {
        section("NEW CARDS") {
            VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("Learning steps")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        Text("e.g. 1m 10m 1d")
                            .font(.system(size: 11.5, design: .monospaced))
                            .foregroundStyle(RemColor.faint)
                    }
                    Text("Intervals a new card steps through before graduating. Space-separated.")
                        .font(.system(size: 12))
                        .foregroundStyle(RemColor.muted)

                    TextField("1m 10m", text: $learnSteps)
                        .font(.system(size: 14, design: .monospaced))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .padding(10)
                        .background(RemColor.bgInset)
                        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .strokeBorder(RemColor.borderStrong, lineWidth: 1)
                        )

                    stepChipsRow(raw: learnSteps, isLapse: false)
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Insertion order")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("Order new cards enter the queue.")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer(minLength: 4)
                    Picker("Insertion order", selection: $insertionOrder) {
                        Text("Seq").tag(InsertionOrderDto.sequential)
                        Text("Random").tag(InsertionOrderDto.random)
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 148)
                }
                .padding(14)
            }
        }
    }

    private var lapsesSection: some View {
        section("LAPSES") {
            VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("Relearn steps")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        Text("e.g. 10m")
                            .font(.system(size: 11.5, design: .monospaced))
                            .foregroundStyle(RemColor.faint)
                    }
                    Text("Steps a lapsed card relearns through. Space-separated.")
                        .font(.system(size: 12))
                        .foregroundStyle(RemColor.muted)

                    TextField("10m", text: $relearnSteps)
                        .font(.system(size: 14, design: .monospaced))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .padding(10)
                        .background(RemColor.bgInset)
                        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .strokeBorder(RemColor.borderStrong, lineWidth: 1)
                        )

                    stepChipsRow(raw: relearnSteps, isLapse: true)
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Minimum interval")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("Floor for intervals after a lapse.")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer(minLength: 4)
                    RemStepperControl(
                        value: $minimumInterval,
                        range: 1...365,
                        step: 1,
                        label: "Minimum interval",
                        formattedValue: "\(minimumInterval)d"
                    )
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Leech threshold")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("Lapses before a card is flagged a leech.")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer(minLength: 4)
                    RemStepperControl(
                        value: $leechThreshold,
                        range: 1...99,
                        step: 1,
                        label: "Leech threshold",
                        formattedValue: "\(leechThreshold)"
                    )
                }
                .padding(14)

                Divider().overlay(RemColor.border)

                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Leech action")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(RemColor.text)
                        Text("What happens when a leech is found.")
                            .font(.system(size: 12))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer(minLength: 4)
                    Picker("Leech action", selection: $leechAction) {
                        Text("Tag").tag(LeechActionDto.tag)
                        Text("Suspend").tag(LeechActionDto.suspend)
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 148)
                }
                .padding(14)
            }
        }
    }

    private var dangerZoneSection: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text("DANGER ZONE")
                .font(.system(size: 11, weight: .bold))
                .tracking(0.8)
                .foregroundStyle(RemColor.danger)
                .padding(.horizontal, 2)

            HStack(alignment: .center, spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Delete this deck")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(RemColor.text)
                    Text("Permanently removes the deck and all its cards.")
                        .font(.system(size: 12))
                        .foregroundStyle(RemColor.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 4)
                Button(role: .destructive) {
                    showDeleteConfirm = true
                } label: {
                    Text("Delete")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(RemColor.danger)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 7)
                        .background(RemColor.againBg)
                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .strokeBorder(RemColor.danger.opacity(0.3), lineWidth: 1)
                        )
                }
            }
            .padding(14)
            .background(RemColor.surface)
            .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                    .strokeBorder(RemColor.danger.opacity(0.28), lineWidth: 1)
            )
        }
    }

    // MARK: - Helpers

    @ViewBuilder
    private func stepChipsRow(raw: String, isLapse: Bool) -> some View {
        let tokens = DeckStepParser.parseTokens(raw)
        if !tokens.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(Array(tokens.enumerated()), id: \.offset) { _, token in
                        let valid = DeckStepParser.isValidToken(token)
                        Text(token)
                            .font(.system(size: 11.5, weight: .semibold, design: .monospaced))
                            .foregroundStyle(
                                !valid
                                    ? RemColor.again
                                    : (isLapse ? RemColor.hard : RemColor.accent)
                            )
                            .padding(.horizontal, 8)
                            .padding(.vertical, 4)
                            .background(
                                !valid
                                    ? RemColor.againBg
                                    : (isLapse ? RemColor.hardBg : RemColor.accentTint)
                            )
                            .clipShape(Capsule())
                    }
                }
            }
            .padding(.top, 2)
        }
    }

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title)
                .font(.system(size: 11, weight: .bold))
                .tracking(0.8)
                .foregroundStyle(RemColor.muted)
                .padding(.horizontal, 2)
            content()
                .background(RemColor.surface)
                .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )
        }
    }

    private func seed(from deck: DeckDto) {
        name = deck.name
        selectedColor = deck.color
        initialColor = deck.color
        desiredRetentionPercent = max(70, min(99, Int((deck.settings.desiredRetention * 100.0).rounded())))
        newPerDay = Int(deck.settings.newPerDay)
        maxReviews = Int(deck.settings.maxReviews)
        learnSteps = deck.settings.learnSteps
        insertionOrder = deck.settings.insertionOrder
        relearnSteps = deck.settings.relearnSteps
        minimumInterval = max(1, min(365, Int(deck.settings.minimumInterval)))
        leechThreshold = max(1, min(99, Int(deck.settings.leechThreshold)))
        leechAction = deck.settings.leechAction
        buryRelated = deck.settings.buryRelated
        showTimer = deck.settings.showTimer
        maximumInterval = max(1, deck.settings.maximumInterval)
        isLoaded = true
    }

    private func loadDeck() async {
        if let cached = appState.decks.first(where: { $0.id == deckId }) {
            seed(from: cached)
        }
        do {
            async let deckTask = appState.store.getDeck(id: deckId)
            async let fsrsTask = appState.store.getDeckFsrsStatus(deckId: deckId)
            let (fetchedDeck, status) = try await (deckTask, fsrsTask)
            if let fetchedDeck {
                seed(from: fetchedDeck)
            }
            fsrsStatus = status
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func saveChanges() async {
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedName.isEmpty else { return }
        isSaving = true
        let patch = DeckSettingsPatchDto(
            newPerDay: UInt32(max(0, min(9999, newPerDay))),
            maxReviews: UInt32(max(0, min(9999, maxReviews))),
            learnSteps: learnSteps.trimmingCharacters(in: .whitespacesAndNewlines),
            insertionOrder: insertionOrder,
            relearnSteps: relearnSteps.trimmingCharacters(in: .whitespacesAndNewlines),
            minimumInterval: UInt32(max(1, min(365, minimumInterval))),
            leechThreshold: UInt32(max(1, min(99, leechThreshold))),
            leechAction: leechAction,
            buryRelated: buryRelated,
            showTimer: showTimer,
            desiredRetention: Double(desiredRetentionPercent) / 100.0,
            maximumInterval: max(1, maximumInterval)
        )
        let updated = await appState.updateDeck(
            id: deckId,
            name: trimmedName,
            color: selectedColor,
            settings: patch
        )
        isSaving = false
        if updated != nil {
            dismiss()
        }
    }

    private func optimizeFsrs() async {
        isOptimizing = true
        let updated = await appState.optimizeDeckFsrs(id: deckId)
        if updated != nil {
            fsrsStatus = try? await appState.store.getDeckFsrsStatus(deckId: deckId)
        }
        isOptimizing = false
    }

    private func resetFsrs() async {
        isOptimizing = true
        let updated = await appState.resetDeckFsrs(id: deckId)
        if updated != nil {
            fsrsStatus = try? await appState.store.getDeckFsrsStatus(deckId: deckId)
        }
        isOptimizing = false
    }
}

struct RemStepperControl: View {
    @Binding var value: Int
    let range: ClosedRange<Int>
    let step: Int
    let label: String
    let formattedValue: String

    private var canDecrement: Bool { value > range.lowerBound }
    private var canIncrement: Bool { value < range.upperBound }

    var body: some View {
        HStack(spacing: 0) {
            Button {
                value = max(range.lowerBound, value - step)
            } label: {
                Image(systemName: "minus")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(canDecrement ? RemColor.text : RemColor.faint)
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .disabled(!canDecrement)
            .accessibilityLabel("Decrease \(label)")

            Text(formattedValue)
                .font(.system(size: 13, weight: .semibold, design: .monospaced))
                .foregroundStyle(RemColor.text)
                .frame(minWidth: 44)
                .multilineTextAlignment(.center)
                .monospacedDigit()

            Button {
                value = min(range.upperBound, value + step)
            } label: {
                Image(systemName: "plus")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(canIncrement ? RemColor.text : RemColor.faint)
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .disabled(!canIncrement)
            .accessibilityLabel("Increase \(label)")
        }
        .background(RemColor.bgInset)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(RemColor.borderStrong, lineWidth: 1)
        )
    }
}
