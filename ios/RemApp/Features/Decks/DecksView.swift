import SwiftUI

struct DecksView: View {
    @Bindable var appState: AppState
    @State private var searchText: String = ""
    @State private var cardHits: [CardSearchHitDto] = []
    @State private var isSearching: Bool = false
    @State private var searchToken: Int = 0
    @State private var deckPendingDelete: DeckSummaryDto?

    private var trimmedSearch: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var allDeckSummaries: [DeckSummaryDto] {
        appState.todaySummary?.decks ?? []
    }

    private var filteredDecks: [DeckSummaryDto] {
        let query = trimmedSearch.lowercased()
        guard !query.isEmpty else { return allDeckSummaries }
        return allDeckSummaries.filter { $0.name.lowercased().contains(query) }
    }

    private func rolledUpTotals(for deck: DeckSummaryDto) -> (due: UInt32, newDue: UInt32, total: UInt32) {
        let descendants = allDeckSummaries.filter {
            $0.id == deck.id || DeckTreeBuilder.isDescendantOrSelf($0.name, of: deck.name)
        }
        return (
            due: descendants.reduce(0) { $0 + $1.dueCount },
            newDue: descendants.reduce(0) { $0 + $1.newDueCount },
            total: descendants.reduce(0) { $0 + $1.totalCards }
        )
    }

    private var treeNodes: [DeckTreeNode] {
        DeckTreeBuilder.flatten(
            DeckTreeBuilder.build(from: allDeckSummaries),
            collapsed: appState.collapsedDeckPaths
        )
    }

    private func toggleCollapse(_ path: String) {
        withAnimation(.easeOut(duration: 0.16)) {
            appState.toggleDeckCollapse(path)
        }
    }

    var body: some View {
        NavigationStack {
            Group {
                if filteredDecks.isEmpty && cardHits.isEmpty && !trimmedSearch.isEmpty && !isSearching {
                    ContentUnavailableView.search(text: searchText)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(RemColor.groupedBg)
                } else if filteredDecks.isEmpty && trimmedSearch.isEmpty {
                    ContentUnavailableView {
                        Label("No decks yet", systemImage: "rectangle.stack")
                    } description: {
                        Text("Create a deck to start organizing your cards.")
                    } actions: {
                        Button("Create new deck") {
                            appState.showNewDeckSheet = true
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(RemColor.accent)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(RemColor.groupedBg)
                } else {
                    List {
                        if trimmedSearch.isEmpty && !treeNodes.isEmpty {
                            Section {
                                ForEach(treeNodes) { node in
                                    if let deck = node.deck {
                                        let isCollapsed = appState.collapsedDeckPaths.contains(node.path)
                                        HStack(spacing: 4) {
                                            if !node.children.isEmpty {
                                                Button {
                                                    toggleCollapse(node.path)
                                                } label: {
                                                    Image(systemName: isCollapsed ? "chevron.right" : "chevron.down")
                                                        .font(.system(size: 11, weight: .bold))
                                                        .foregroundStyle(RemColor.muted)
                                                        .frame(width: 28, height: 34)
                                                        .contentShape(Rectangle())
                                                }
                                                .buttonStyle(.plain)
                                                .padding(.leading, CGFloat(min(node.depth, 4)) * 14)
                                                .accessibilityLabel(isCollapsed ? "Expand \(node.label)" : "Collapse \(node.label)")
                                            }
                                            NavigationLink(value: deck.id) {
                                                DeckSummaryRow(
                                                    deck: deck,
                                                    showChevron: false,
                                                    displayName: node.label,
                                                    depth: node.children.isEmpty ? node.depth : 0,
                                                    dueOverride: node.dueCount,
                                                    newDueOverride: node.newDueCount,
                                                    totalOverride: node.totalCards
                                                )
                                            }
                                        }
                                        .swipeActions(edge: .leading, allowsFullSwipe: true) {
                                            if node.dueCount > 0 {
                                                Button {
                                                    appState.startStudy(deckId: deck.id)
                                                } label: {
                                                    Label("Study", systemImage: "play.fill")
                                                }
                                                .tint(RemColor.accent)
                                            }
                                        }
                                        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                                            Button(role: .destructive) {
                                                deckPendingDelete = deck
                                            } label: {
                                                Label("Delete", systemImage: "trash")
                                            }

                                            Button {
                                                appState.openEditDeck(deckId: deck.id)
                                            } label: {
                                                Label("Edit", systemImage: "slider.horizontal.3")
                                            }
                                            .tint(RemColor.accent)
                                        }
                                        .contextMenu {
                                            Button {
                                                appState.openEditDeck(deckId: deck.id)
                                            } label: {
                                                Label("Edit deck", systemImage: "slider.horizontal.3")
                                            }
                                            Button(role: .destructive) {
                                                deckPendingDelete = deck
                                            } label: {
                                                Label("Delete deck", systemImage: "trash")
                                            }
                                        }
                                    } else {
                                        VirtualDeckFolderRow(
                                            node: node,
                                            isCollapsed: appState.collapsedDeckPaths.contains(node.path),
                                            insetPadding: false
                                        ) {
                                            toggleCollapse(node.path)
                                        }
                                    }
                                }
                            }
                            .listRowBackground(RemColor.surface)
                            .listRowSeparatorTint(RemColor.border)
                        } else if !trimmedSearch.isEmpty && !filteredDecks.isEmpty {
                            Section("Decks") {
                                ForEach(filteredDecks, id: \.id) { deck in
                                    let totals = rolledUpTotals(for: deck)
                                    NavigationLink(value: deck.id) {
                                        DeckSummaryRow(
                                            deck: deck,
                                            showChevron: false,
                                            dueOverride: totals.due,
                                            newDueOverride: totals.newDue,
                                            totalOverride: totals.total
                                        )
                                    }
                                    .swipeActions(edge: .leading, allowsFullSwipe: true) {
                                        if totals.due > 0 {
                                            Button {
                                                appState.startStudy(deckId: deck.id)
                                            } label: {
                                                Label("Study", systemImage: "play.fill")
                                            }
                                            .tint(RemColor.accent)
                                        }
                                    }
                                    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                                        Button(role: .destructive) {
                                            deckPendingDelete = deck
                                        } label: {
                                            Label("Delete", systemImage: "trash")
                                        }

                                        Button {
                                            appState.openEditDeck(deckId: deck.id)
                                        } label: {
                                            Label("Edit", systemImage: "slider.horizontal.3")
                                        }
                                        .tint(RemColor.accent)
                                    }
                                    .contextMenu {
                                        Button {
                                            appState.openEditDeck(deckId: deck.id)
                                        } label: {
                                            Label("Edit deck", systemImage: "slider.horizontal.3")
                                        }
                                        Button(role: .destructive) {
                                            deckPendingDelete = deck
                                        } label: {
                                            Label("Delete deck", systemImage: "trash")
                                        }
                                    }
                                }
                            }
                            .listRowBackground(RemColor.surface)
                            .listRowSeparatorTint(RemColor.border)
                        }

                        if !trimmedSearch.isEmpty && !cardHits.isEmpty {
                            Section("Cards") {
                                ForEach(cardHits, id: \.card.id) { hit in
                                    Button {
                                        appState.openQuickCapture(
                                            deckId: hit.card.deckId,
                                            editingCard: hit.card
                                        ) {
                                            Task { await runCardSearch(trimmedSearch) }
                                        }
                                    } label: {
                                        VStack(alignment: .leading, spacing: 6) {
                                            HStack(spacing: 6) {
                                                Circle()
                                                    .fill(Color(hexString: hit.deckColor))
                                                    .frame(width: 7, height: 7)
                                                Text(hit.deckName)
                                                    .font(.caption.weight(.medium))
                                                    .foregroundStyle(RemColor.muted)
                                                Spacer()
                                                if hit.card.suspended {
                                                    RemPillBadge(text: "Suspended", tone: .neutral)
                                                }
                                            }
                                            let preview = MarkdownBlockParser.plainTextPreview(hit.card.front)
                                            Text(preview.isEmpty ? hit.card.front : preview)
                                                .font(.subheadline.weight(.medium))
                                                .foregroundStyle(hit.card.suspended ? RemColor.muted : RemColor.text)
                                                .lineLimit(2)
                                                .multilineTextAlignment(.leading)
                                        }
                                        .padding(.vertical, 2)
                                        .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                            .listRowBackground(RemColor.surface)
                            .listRowSeparatorTint(RemColor.border)
                        }

                        if trimmedSearch.isEmpty {
                            Section {
                                Button {
                                    appState.showNewDeckSheet = true
                                } label: {
                                    Label("Create new deck", systemImage: "plus")
                                        .font(.body.weight(.medium))
                                        .foregroundStyle(RemColor.accent)
                                }
                            }
                            .listRowBackground(RemColor.surface)
                        }
                    }
                    .listStyle(.insetGrouped)
                    .scrollContentBackground(.hidden)
                    .background(RemColor.groupedBg)
                }
            }
            .navigationTitle("Decks")
            .searchable(text: $searchText, prompt: "Search decks and cards")
            .task(id: trimmedSearch) {
                await runCardSearch(trimmedSearch, debounce: true)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        appState.openQuickCapture()
                    } label: {
                        Image(systemName: "plus")
                    }
                    .accessibilityLabel("New card")
                }
            }
            .navigationDestination(for: String.self) { deckId in
                DeckDetailView(appState: appState, deckId: deckId)
            }
            .alert(
                "Delete deck?",
                isPresented: Binding(
                    get: { deckPendingDelete != nil },
                    set: { if !$0 { deckPendingDelete = nil } }
                )
            ) {
                if let deck = deckPendingDelete {
                    Button("Delete", role: .destructive) {
                        deckPendingDelete = nil
                        Task {
                            await appState.deleteDeck(id: deck.id)
                            if !trimmedSearch.isEmpty {
                                await runCardSearch(trimmedSearch)
                            }
                        }
                    }
                }
                Button("Cancel", role: .cancel) {
                    deckPendingDelete = nil
                }
            } message: {
                Text("This deletes the deck and all its cards locally and on synced devices.")
            }
        }
    }

    private func runCardSearch(_ query: String, debounce: Bool = false) async {
        searchToken += 1
        let token = searchToken
        guard !query.isEmpty else {
            isSearching = false
            cardHits = []
            return
        }
        isSearching = true
        defer {
            if token == searchToken {
                isSearching = false
            }
        }
        if debounce {
            try? await Task.sleep(for: .milliseconds(120))
            guard !Task.isCancelled && token == searchToken else { return }
        }
        do {
            let results = try await appState.store.searchCards(query: query, limit: 40)
            guard !Task.isCancelled && token == searchToken else { return }
            cardHits = results
        } catch {
            guard !Task.isCancelled && token == searchToken else { return }
            cardHits = []
        }
    }
}
