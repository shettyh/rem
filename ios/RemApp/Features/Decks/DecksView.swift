import SwiftUI

struct DecksView: View {
    @Bindable var appState: AppState
    @State private var searchText: String = ""
    @State private var cardHits: [CardSearchHitDto] = []
    @State private var deckPendingDelete: DeckSummaryDto?

    private var trimmedSearch: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var filteredDecks: [DeckSummaryDto] {
        let summaries = appState.todaySummary?.decks ?? []
        let query = trimmedSearch.lowercased()
        guard !query.isEmpty else { return summaries }
        return summaries.filter { $0.name.lowercased().contains(query) }
    }

    var body: some View {
        NavigationStack {
            Group {
                if filteredDecks.isEmpty && cardHits.isEmpty && !trimmedSearch.isEmpty {
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
                        if !filteredDecks.isEmpty {
                            Section(trimmedSearch.isEmpty ? "" : "Decks") {
                                ForEach(filteredDecks, id: \.id) { deck in
                                    NavigationLink(value: deck.id) {
                                        DeckSummaryRow(deck: deck, showChevron: false)
                                    }
                                    .swipeActions(edge: .leading, allowsFullSwipe: true) {
                                        if deck.dueCount > 0 {
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
                await runCardSearch(trimmedSearch)
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
                        Task { await appState.deleteDeck(id: deck.id) }
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

    private func runCardSearch(_ query: String) async {
        guard !query.isEmpty else {
            cardHits = []
            return
        }
        do {
            cardHits = try await appState.store.searchCards(query: query, limit: 40)
        } catch {
            cardHits = []
        }
    }
}
