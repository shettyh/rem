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
    @State private var maxReviews: Int = 9999
    @State private var learnSteps: String = "1m 10m"
    @State private var insertionOrder: InsertionOrderDto = .sequential
    @State private var relearnSteps: String = "10m"
    @State private var minimumInterval: Int = 1
    @State private var leechThreshold: Int = 8
    @State private var leechAction: LeechActionDto = .tag
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
            Form {
                generalSection
                dailyLimitsSection
                newCardsSection
                lapsesSection
                dangerZoneSection
            }
            .scrollContentBackground(.hidden)
            .scrollDismissesKeyboard(.interactively)
            .background(RemColor.groupedBg)
            .navigationTitle("Deck options")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(isSaving ? "Saving…" : "Save") {
                        Task { await saveChanges() }
                    }
                    .fontWeight(.semibold)
                    .disabled(!canSave)
                }
            }
        }
        .tint(RemColor.accent)
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
        Section("General") {
            TextField("Deck name", text: $name)
                .foregroundStyle(RemColor.text)

            VStack(alignment: .leading, spacing: 10) {
                Text("Color")
                    .font(.subheadline)
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
                        .buttonStyle(.plain)
                        .accessibilityLabel("Color \(hex)")
                        .accessibilityAddTraits(isSelected ? .isSelected : [])
                    }
                }
                .padding(.vertical, 4)
            }

            Stepper(value: $desiredRetentionPercent, in: 70...99, step: 1) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text("Desired retention")
                            .foregroundStyle(RemColor.text)
                        Spacer()
                        Text("\(desiredRetentionPercent)%")
                            .font(.subheadline.weight(.semibold).monospacedDigit())
                            .foregroundStyle(RemColor.accent)
                    }
                    Text("Higher values schedule more frequent reviews.")
                        .font(.caption)
                        .foregroundStyle(RemColor.muted)
                }
            }

            personalizedSchedulingRow
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var personalizedSchedulingRow: some View {
        let hasWeights = fsrsStatus?.hasPersonalizedWeights ?? false
        let reviewCount = Int(fsrsStatus?.reviewCount ?? 0)
        let canOptimize = fsrsStatus?.canOptimize ?? false

        return VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Personalized scheduling")
                    .foregroundStyle(RemColor.text)
                Text("\(hasWeights ? "Personalized" : "Default") schedule · \(reviewCount) recorded \(reviewCount == 1 ? "review" : "reviews")")
                    .font(.caption)
                    .foregroundStyle(RemColor.muted)
                if !canOptimize {
                    Text("Optimize after reviewing a card on a later day.")
                        .font(.caption)
                        .foregroundStyle(RemColor.faint)
                }
            }

            HStack(spacing: 10) {
                Button {
                    Task { await optimizeFsrs() }
                } label: {
                    HStack(spacing: 6) {
                        if isOptimizing {
                            ProgressView()
                                .controlSize(.small)
                        }
                        Text(isOptimizing ? "Optimizing…" : "Optimize")
                    }
                }
                .buttonStyle(.borderedProminent)
                .tint(RemColor.accent)
                .controlSize(.small)
                .disabled(!canOptimize || isOptimizing || isSaving)

                if hasWeights {
                    Button("Reset") {
                        Task { await resetFsrs() }
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(isOptimizing || isSaving)
                }
            }
        }
        .padding(.vertical, 2)
    }

    private var dailyLimitsSection: some View {
        Section("Daily Limits") {
            Stepper(value: $newPerDay, in: 0...9999, step: 5) {
                HStack {
                    Text("New cards/day")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text("\(newPerDay)")
                        .font(.subheadline.weight(.semibold).monospacedDigit())
                        .foregroundStyle(RemColor.muted)
                }
            }

            Stepper(value: $maxReviews, in: 0...9999, step: 10) {
                HStack {
                    Text("Maximum reviews/day")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text("\(maxReviews)")
                        .font(.subheadline.weight(.semibold).monospacedDigit())
                        .foregroundStyle(RemColor.muted)
                }
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var newCardsSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Learning steps")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text("e.g. 1m 10m")
                        .font(.caption.monospaced())
                        .foregroundStyle(RemColor.faint)
                }

                TextField("1m 10m", text: $learnSteps)
                    .font(.subheadline.monospaced())
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .padding(9)
                    .background(RemColor.bgInset)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

                stepChipsRow(raw: learnSteps, isLapse: false)
            }
            .padding(.vertical, 2)

            Picker("Insertion order", selection: $insertionOrder) {
                Text("Sequential").tag(InsertionOrderDto.sequential)
                Text("Random").tag(InsertionOrderDto.random)
            }
        } header: {
            Text("New Cards")
        } footer: {
            Text("Space-separated intervals a new card steps through before graduating.")
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var lapsesSection: some View {
        Section("Lapses") {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Relearn steps")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text("e.g. 10m")
                        .font(.caption.monospaced())
                        .foregroundStyle(RemColor.faint)
                }

                TextField("10m", text: $relearnSteps)
                    .font(.subheadline.monospaced())
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .padding(9)
                    .background(RemColor.bgInset)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

                stepChipsRow(raw: relearnSteps, isLapse: true)
            }
            .padding(.vertical, 2)

            Stepper(value: $minimumInterval, in: 1...365, step: 1) {
                HStack {
                    Text("Minimum interval")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text("\(minimumInterval)d")
                        .font(.subheadline.weight(.semibold).monospacedDigit())
                        .foregroundStyle(RemColor.muted)
                }
            }

            Stepper(value: $leechThreshold, in: 1...99, step: 1) {
                HStack {
                    Text("Leech threshold")
                        .foregroundStyle(RemColor.text)
                    Spacer()
                    Text("\(leechThreshold)")
                        .font(.subheadline.weight(.semibold).monospacedDigit())
                        .foregroundStyle(RemColor.muted)
                }
            }

            Picker("Leech action", selection: $leechAction) {
                Text("Tag").tag(LeechActionDto.tag)
                Text("Suspend").tag(LeechActionDto.suspend)
            }
        }
        .listRowBackground(RemColor.surface)
        .listRowSeparatorTint(RemColor.border)
    }

    private var dangerZoneSection: some View {
        Section {
            Button(role: .destructive) {
                showDeleteConfirm = true
            } label: {
                Label("Delete this deck", systemImage: "trash")
            }
        } footer: {
            Text("Permanently removes the deck and all its cards locally and on synced devices.")
        }
        .listRowBackground(RemColor.surface)
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
                            .font(.caption.weight(.semibold).monospaced())
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
