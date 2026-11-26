import SwiftUI

struct DeckDetailView: View {
    @Bindable var appState: AppState
    let deckId: String

    @Environment(\.dismiss) private var dismiss
    @State private var cards: [CardDto] = []
    @State private var searchText: String = ""
    @State private var selectedTag: String = "All"
    @State private var showCustomStudy: Bool = false
    @State private var showDeleteConfirm: Bool = false
    @State private var cardPendingDelete: CardDto?

    private var deckSummary: DeckSummaryDto? {
        appState.todaySummary?.decks.first { $0.id == deckId }
    }

    private var allTags: [String] {
        var set = Set<String>()
        for card in cards {
            for tag in card.tags { set.insert(tag) }
        }
        return ["All"] + set.sorted()
    }

    private var filteredCards: [CardDto] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return cards.filter { card in
            let matchesTag = selectedTag == "All" || card.tags.contains(selectedTag)
            let matchesQuery = query.isEmpty
                || card.front.lowercased().contains(query)
                || card.back.lowercased().contains(query)
                || card.tags.contains(where: { $0.lowercased().contains(query) })
            return matchesTag && matchesQuery
        }
    }

    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(alignment: .center, spacing: 10) {
                        if let colorHex = deckSummary?.color {
                            Circle()
                                .fill(Color(hexString: colorHex))
                                .frame(width: 9, height: 9)
                        }

                        if let summary = deckSummary {
                            let cardLabel = "\(cards.count) \(cards.count == 1 ? "card" : "cards")"
                            let newPart = summary.newDueCount > 0 ? " · \(summary.newDueCount) new" : ""
                            let duePart = summary.dueCount > 0 ? " · \(summary.dueCount) due" : (cards.isEmpty ? "" : " · All caught up")
                            Text("\(cardLabel)\(duePart)\(newPart)")
                                .font(.footnote)
                                .foregroundStyle(RemColor.muted)
                        }

                        Spacer(minLength: 8)

                        if let due = deckSummary?.dueCount, due > 0 {
                            Button("Study (\(due))") {
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

            if filteredCards.isEmpty {
                Section {
                    Group {
                        if !searchText.isEmpty {
                            ContentUnavailableView.search(text: searchText)
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
                    .padding(.vertical, 12)
                }
                .listRowBackground(RemColor.surface)
            } else {
                Section {
                    ForEach(filteredCards, id: \.id) { card in
                        CardBrowserRow(card: card) {
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
                }
                .listRowBackground(RemColor.surface)
                .listRowSeparatorTint(RemColor.border)
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(RemColor.groupedBg)
        .navigationTitle(deckSummary?.name ?? "Deck")
        .navigationBarTitleDisplayMode(.large)
        .searchable(text: $searchText, prompt: "Filter cards in deck")
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Menu {
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
                        Label("Delete deck", systemImage: "trash")
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
        .onAppear {
            appState.activeDeckId = deckId
            Task { await loadCards() }
        }
        .onDisappear {
            if appState.activeDeckId == deckId {
                appState.activeDeckId = nil
            }
        }
        .sheet(isPresented: $showCustomStudy) {
            CustomStudySheet(
                appState: appState,
                deckId: deckId,
                deckName: deckSummary?.name ?? "Deck"
            )
        }
        .alert("Delete deck?", isPresented: $showDeleteConfirm) {
            Button("Delete", role: .destructive) {
                Task {
                    await appState.deleteDeck(id: deckId)
                    dismiss()
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This deletes the deck and all its cards locally and on synced devices.")
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
    let onTap: () -> Void

    var body: some View {
        let preview = MarkdownBlockParser.plainTextPreview(card.front)
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 8) {
                Text(preview.isEmpty ? card.front : preview)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(card.suspended ? RemColor.muted : RemColor.text)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)

                HStack(spacing: 8) {
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
