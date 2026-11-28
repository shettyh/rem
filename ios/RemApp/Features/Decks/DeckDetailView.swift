import SwiftUI

struct DeckDetailView: View {
    @Bindable var appState: AppState
    let deckId: String
    var onNavigateToDeck: ((String) -> Void)? = nil

    @Environment(\.dismiss) private var dismiss
    @State private var cards: [CardDto] = []
    @State private var hasLoadedCards: Bool = false
    @State private var searchText: String = ""
    @State private var selectedTag: String = "All"
    @State private var showCustomStudy: Bool = false
    @State private var pendingCustomStudy: CustomStudySpec?
    @State private var showDeleteConfirm: Bool = false
    @State private var cardPendingDelete: CardDto?
    @State private var showAddSubdeckAlert: Bool = false
    @State private var newSubdeckName: String = ""

    private var allSummaries: [DeckSummaryDto] {
        appState.todaySummary?.decks ?? []
    }

    private var deckSummary: DeckSummaryDto? {
        allSummaries.first { $0.id == deckId }
    }

    private var treeNode: DeckTreeNode? {
        let roots = DeckTreeBuilder.build(from: allSummaries)
        return DeckTreeBuilder.findNode(deckId: deckId, in: roots)
    }

    private var directSubdecks: [DeckTreeBuilder.NavigableSubdeck] {
        guard let node = treeNode else { return [] }
        return DeckTreeBuilder.navigableChildren(of: node)
    }

    private var ancestors: [(path: String, label: String, deckId: String?)] {
        guard let name = deckSummary?.name else { return [] }
        let byName = Dictionary(
            allSummaries.map { ($0.name, $0.id) },
            uniquingKeysWith: { first, _ in first }
        )
        return DeckTreeBuilder.ancestorPaths(name).map { path in
            (path: path, label: DeckTreeBuilder.leafName(path), deckId: byName[path])
        }
    }

    private var leafTitle: String {
        DeckTreeBuilder.leafName(deckSummary?.name ?? "Deck")
    }

    private var hasSubdecks: Bool {
        !directSubdecks.isEmpty
    }

    private var deleteCopy: DeckDeleteCopy {
        if let node = treeNode {
            return DeckTreeBuilder.describeDelete(node: node)
        }
        return DeckTreeBuilder.describeDelete(
            deckId: deckId,
            deckName: deckSummary?.name ?? "Deck",
            allSummaries: allSummaries
        )
    }

    private var rolledUpDueCount: UInt32 {
        treeNode?.dueCount ?? deckSummary?.dueCount ?? 0
    }

    private var rolledUpNewDueCount: UInt32 {
        treeNode?.newDueCount ?? deckSummary?.newDueCount ?? 0
    }

    private var rolledUpTotalCards: Int {
        if let node = treeNode {
            let directDelta = hasLoadedCards
                ? cards.count - Int(deckSummary?.totalCards ?? 0)
                : 0
            return max(0, Int(node.totalCards) + directDelta)
        }
        return hasLoadedCards ? cards.count : Int(deckSummary?.totalCards ?? 0)
    }

    private var allTags: [String] {
        var set = Set<String>()
        for card in cards {
            for tag in card.tags { set.insert(tag) }
        }
        return ["All"] + set.sorted()
    }

    private var trimmedSearchText: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var filteredCards: [CardDto] {
        cards.filter { card in
            let matchesTag = selectedTag == "All" || card.tags.contains(selectedTag)
            let matchesQuery = CardSearchResultMeta.matchesInDeckFilter(card: card, query: trimmedSearchText)
            return matchesTag && matchesQuery
        }
    }

    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 12) {
                    if !ancestors.isEmpty {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 5) {
                                ForEach(Array(ancestors.enumerated()), id: \.element.path) { idx, crumb in
                                    if idx > 0 {
                                        Image(systemName: "chevron.right")
                                            .font(.system(size: 9, weight: .semibold))
                                            .foregroundStyle(RemColor.faint)
                                    }
                                    if let parentId = crumb.deckId, let onNavigateToDeck {
                                        Button {
                                            onNavigateToDeck(parentId)
                                        } label: {
                                            Text(crumb.label)
                                                .font(.caption.weight(.medium))
                                                .foregroundStyle(RemColor.muted)
                                        }
                                        .buttonStyle(.borderless)
                                    } else {
                                        Text(crumb.label)
                                            .font(.caption.weight(.medium))
                                            .foregroundStyle(RemColor.muted)
                                    }
                                }
                            }
                        }
                    }

                    HStack(alignment: .center, spacing: 10) {
                        if hasSubdecks {
                            Image(systemName: "folder")
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(RemColor.muted)
                        } else if let colorHex = deckSummary?.color {
                            Circle()
                                .fill(Color(hexString: colorHex))
                                .frame(width: 9, height: 9)
                        }

                        if deckSummary != nil {
                            let total = rolledUpTotalCards
                            let cardLabel = "\(total) \(total == 1 ? "card" : "cards")"
                            let newPart = rolledUpNewDueCount > 0 ? " · \(rolledUpNewDueCount) new" : ""
                            let duePart = rolledUpDueCount > 0 ? " · \(rolledUpDueCount) due" : (total == 0 ? "" : " · All caught up")
                            Text("\(cardLabel)\(duePart)\(newPart)")
                                .font(.footnote)
                                .foregroundStyle(RemColor.muted)
                        }

                        Spacer(minLength: 8)

                        if rolledUpDueCount > 0 {
                            Button("Study (\(rolledUpDueCount))") {
                                appState.startStudy(deckId: deckId)
                            }
                            .font(.subheadline.weight(.semibold))
                            .buttonStyle(.borderedProminent)
                            .tint(RemColor.accent)
                        }
                    }

                    if allTags.count > 1 {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 6) {
                                ForEach(allTags, id: \.self) { tag in
                                    let active = selectedTag == tag
                                    Button {
                                        selectedTag = tag
                                    } label: {
                                        Text(tag == "All" ? "All" : "#\(tag)")
                                            .font(.caption.weight(.medium))
                                            .foregroundStyle(active ? RemColor.bg : RemColor.muted)
                                            .padding(.horizontal, 11)
                                            .padding(.vertical, 6)
                                            .background(active ? RemColor.text : RemColor.surface)
                                            .clipShape(Capsule())
                                            .overlay(
                                                Capsule().strokeBorder(active ? RemColor.text : RemColor.border, lineWidth: 1)
                                            )
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                        }
                    }
                }
                .listRowInsets(EdgeInsets(top: 4, leading: 2, bottom: 4, trailing: 2))
                .listRowBackground(Color.clear)
            }

            if !directSubdecks.isEmpty {
                Section("Subdecks (\(directSubdecks.count))") {
                    ForEach(directSubdecks) { entry in
                        let child = entry.node
                        let sub = entry.deck
                        NavigationLink(value: sub.id) {
                            DeckSummaryRow(
                                deck: sub,
                                showChevron: false,
                                displayName: entry.displayLabel,
                                hasSubdecks: !child.children.isEmpty,
                                subdeckCount: child.subdeckCount,
                                dueOverride: child.dueCount,
                                newDueOverride: child.newDueCount,
                                totalOverride: child.totalCards,
                                iconSlotWidth: 16
                            )
                        }
                        .swipeActions(edge: .leading, allowsFullSwipe: true) {
                            if child.dueCount > 0 {
                                Button {
                                    appState.startStudy(deckId: sub.id)
                                } label: {
                                    Label("Study", systemImage: "play.fill")
                                }
                                .tint(RemColor.accent)
                            }
                        }
                        .contextMenu {
                            if child.dueCount > 0 {
                                Button {
                                    appState.startStudy(deckId: sub.id)
                                } label: {
                                    Label("Study due cards (\(child.dueCount))", systemImage: "play.fill")
                                }
                            }
                            Button {
                                appState.openEditDeck(deckId: sub.id)
                            } label: {
                                Label("Edit deck", systemImage: "slider.horizontal.3")
                            }
                        }
                    }
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)
            }

            if filteredCards.isEmpty {
                Section {
                    Group {
                        if !trimmedSearchText.isEmpty {
                            ContentUnavailableView.search(text: trimmedSearchText)
                        } else if cards.isEmpty && hasSubdecks {
                            HStack(spacing: 12) {
                                Text(
                                    rolledUpTotalCards > 0
                                        ? "All \(rolledUpTotalCards) \(rolledUpTotalCards == 1 ? "card lives" : "cards live") in the subdecks above."
                                        : "No cards in this deck or its subdecks yet."
                                )
                                .font(.footnote)
                                .foregroundStyle(RemColor.muted)
                                Spacer(minLength: 8)
                                Button("Add card") {
                                    appState.openQuickCapture(deckId: deckId) {
                                        Task { await loadCards() }
                                    }
                                }
                                .font(.footnote.weight(.semibold))
                                .tint(RemColor.accent)
                            }
                        } else if cards.isEmpty {
                            ContentUnavailableView {
                                Label("No cards in this deck", systemImage: "rectangle.on.rectangle")
                            } description: {
                                Text("Tap + above to add your first card.")
                            } actions: {
                                Button("New card") {
                                    appState.openQuickCapture(deckId: deckId) {
                                        Task { await loadCards() }
                                    }
                                }
                                .buttonStyle(.borderedProminent)
                                .tint(RemColor.accent)
                            }
                        } else {
                            ContentUnavailableView {
                                Label("No matching cards", systemImage: "tag")
                            } description: {
                                Text("Try clearing your tag filter.")
                            } actions: {
                                Button("Show all") {
                                    selectedTag = "All"
                                }
                                .buttonStyle(.bordered)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, cards.isEmpty && hasSubdecks && searchText.isEmpty ? 2 : 12)
                }
                .listRowBackground(RemColor.surface)
            } else {
                Section {
                    ForEach(filteredCards, id: \.id) { card in
                        CardBrowserRow(card: card, searchQuery: trimmedSearchText) {
                            appState.openQuickCapture(deckId: deckId, editingCard: card) {
                                Task { await loadCards() }
                            }
                        }
                        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                            Button(role: .destructive) {
                                cardPendingDelete = card
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }

                            Button {
                                Task { await toggleSuspend(card) }
                            } label: {
                                Label(
                                    card.suspended ? "Unsuspend" : "Suspend",
                                    systemImage: card.suspended ? "play.circle" : "pause.circle"
                                )
                            }
                            .tint(card.suspended ? RemColor.good : RemColor.hard)
                        }
                    }
                } header: {
                    if hasSubdecks {
                        Text("Cards in this deck (\(cards.count))")
                    }
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(RemColor.groupedBg)
        .navigationTitle(leafTitle)
        .navigationBarTitleDisplayMode(.large)
        .searchable(text: $searchText, prompt: "Filter cards in deck")
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Menu {
                    Button {
                        newSubdeckName = ""
                        showAddSubdeckAlert = true
                    } label: {
                        Label("New subdeck", systemImage: "folder.badge.plus")
                    }

                    Button {
                        showCustomStudy = true
                    } label: {
                        Label("Custom study", systemImage: "sparkles")
                    }

                    Button {
                        appState.openEditDeck(deckId: deckId) {
                            dismiss()
                        }
                    } label: {
                        Label("Edit deck", systemImage: "slider.horizontal.3")
                    }

                    Button(role: .destructive) {
                        showDeleteConfirm = true
                    } label: {
                        Label(
                            hasSubdecks ? "Delete deck & subdecks" : "Delete deck",
                            systemImage: "trash"
                        )
                    }
                } label: {
                    Image(systemName: "ellipsis")
                }
                .accessibilityLabel("Deck actions")

                Button {
                    appState.openQuickCapture(deckId: deckId) {
                        Task { await loadCards() }
                    }
                } label: {
                    Image(systemName: "plus")
                }
                .accessibilityLabel("New card")
            }
        }
        .alert("New subdeck", isPresented: $showAddSubdeckAlert) {
            TextField("Subdeck name", text: $newSubdeckName)
            Button("Create") {
                let child = newSubdeckName.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !child.isEmpty, let parentName = deckSummary?.name else { return }
                let color = deckSummary?.color ?? RemColor.deckPalette[0]
                Task {
                    _ = await appState.createDeck(
                        name: "\(parentName)\(DeckTreeBuilder.separator)\(child)",
                        color: color
                    )
                }
            }
            .disabled(newSubdeckName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Creates a subdeck inside \"\(leafTitle)\".")
        }
        .onChange(of: deckSummary?.id) { _, newValue in
            if newValue == nil && appState.todaySummary != nil {
                dismiss()
            }
        }
        .onAppear {
            appState.activeDeckId = deckId
            Task { await loadCards() }
        }
        .onDisappear {
            if appState.activeDeckId == deckId {
                appState.activeDeckId = nil
            }
        }
        .sheet(
            isPresented: $showCustomStudy,
            onDismiss: {
                if let spec = pendingCustomStudy {
                    pendingCustomStudy = nil
                    appState.startCustomStudy(
                        deckId: deckId,
                        mode: spec.mode,
                        amount: spec.amount
                    )
                }
            }
        ) {
            CustomStudySheet(deckName: deckSummary?.name ?? "Deck") { spec in
                pendingCustomStudy = spec
            }
        }
        .alert(deleteCopy.title, isPresented: $showDeleteConfirm) {
            Button(deleteCopy.confirmLabel, role: .destructive) {
                Task {
                    await appState.deleteDeck(id: deckId)
                    dismiss()
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(deleteCopy.message)
        }
        .alert(
            "Delete card?",
            isPresented: Binding(
                get: { cardPendingDelete != nil },
                set: { if !$0 { cardPendingDelete = nil } }
            )
        ) {
            if let card = cardPendingDelete {
                Button("Delete", role: .destructive) {
                    cardPendingDelete = nil
                    Task { await deleteCard(card) }
                }
            }
            Button("Cancel", role: .cancel) {
                cardPendingDelete = nil
            }
        } message: {
            Text("This permanently deletes the card and its review history.")
        }
    }

    private func loadCards() async {
        do {
            self.cards = try await appState.store.listCards(deckId: deckId)
            self.hasLoadedCards = true
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func toggleSuspend(_ card: CardDto) async {
        do {
            let nextSuspended = !card.suspended
            try await appState.store.updateCard(
                id: card.id,
                front: nil,
                back: nil,
                tags: nil,
                suspended: nextSuspended
            )
            await loadCards()
            await appState.refreshAll()
            appState.showToast(nextSuspended ? "Card suspended" : "Card unsuspended")
            Task { await appState.syncNow(silent: true) }
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }

    private func deleteCard(_ card: CardDto) async {
        do {
            try await appState.store.deleteCard(id: card.id)
            await loadCards()
            await appState.refreshAll()
            appState.showToast("Card deleted")
            Task { await appState.syncNow(silent: true) }
        } catch {
            appState.showToast(error.remDisplayMessage)
        }
    }
}

struct CardBrowserRow: View {
    let card: CardDto
    var searchQuery: String = ""
    let onTap: () -> Void

    var body: some View {
        let preview = MarkdownBlockParser.plainTextPreview(card.front)
        let rationaleMatch = CardSearchResultMeta.rationaleOnlyPreview(card: card, query: searchQuery)
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                Text(preview.isEmpty ? card.front : preview)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(card.suspended ? RemColor.muted : RemColor.text)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)

                if let rationaleMatch {
                    Text(rationaleMatch)
                        .font(.caption.italic())
                        .foregroundStyle(RemColor.muted)
                        .lineLimit(1)
                }

                HStack(spacing: 8) {
                    if let clozeIdx = card.clozeIndex, clozeIdx > 0 {
                        Text(ImageOcclusionParser.hasOcclusion(card.front) ? "IO \(clozeIdx)" : "c\(clozeIdx)")
                            .font(.caption2.weight(.bold).monospaced())
                            .foregroundStyle(RemColor.accent)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(RemColor.accentTint)
                            .clipShape(Capsule())
                    }
                    if rationaleMatch != nil {
                        RemPillBadge(text: "Rationale", tone: .neutral)
                    }
                    if let firstTag = card.tags.first {
                        Text("#\(firstTag)")
                            .font(.caption)
                            .foregroundStyle(RemColor.muted)
                    }
                    if !card.assetHashes.isEmpty {
                        Label("\(card.assetHashes.count)", systemImage: "photo")
                            .font(.caption)
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer()
                    statusPill
                }
            }
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var statusPill: some View {
        let nowMs = RemStore.nowMs()
        if card.suspended {
            return RemPillBadge(text: "Suspended", tone: .neutral)
        }
        if card.state == 1 || card.state == 3 {
            return RemPillBadge(text: "Learning", tone: .hard)
        }
        if card.due <= nowMs {
            return RemPillBadge(text: card.state == 0 ? "New" : "Due", tone: .accent)
        }
        let days = max(1, Int(round(Double(card.due - nowMs) / 86_400_000.0)))
        return RemPillBadge(text: "\(days)d", tone: .neutral)
    }
}
