import SwiftUI

struct DecksView: View {
    @Bindable var appState: AppState
    @State private var searchText: String = ""
    @State private var deckPendingDelete: DeckSummaryDto?

    private var filteredDecks: [DeckSummaryDto] {
        let summaries = appState.todaySummary?.decks ?? []
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return summaries }
        return summaries.filter { $0.name.lowercased().contains(query) }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Text("Decks")
                        .font(.system(size: 28, weight: .bold))
                        .tracking(-0.6)
                        .foregroundStyle(RemColor.text)

                    HStack(spacing: 8) {
                        Image(systemName: "magnifyingglass")
                            .font(.system(size: 14))
                            .foregroundStyle(RemColor.muted)
                        TextField("Search decks…", text: $searchText)
                            .font(.system(size: 14))
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        if !searchText.isEmpty {
                            Button {
                                searchText = ""
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .foregroundStyle(RemColor.faint)
                            }
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(RemColor.bgInset)
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .strokeBorder(RemColor.border, lineWidth: 1)
                    )

                    if filteredDecks.isEmpty {
                        RemCardSurface(padding: 20) {
                            VStack(spacing: 6) {
                                Text(searchText.isEmpty ? "No decks yet" : "No matching decks")
                                    .font(.system(size: 15, weight: .semibold))
                                    .foregroundStyle(RemColor.text)
                                Text(searchText.isEmpty
                                     ? "Create a deck below to organize your cards."
                                     : "Try another search term.")
                                    .font(.system(size: 13))
                                    .foregroundStyle(RemColor.muted)
                            }
                            .frame(maxWidth: .infinity)
                        }
                    } else {
                        VStack(spacing: 0) {
                            ForEach(Array(filteredDecks.enumerated()), id: \.element.id) { idx, deck in
                                NavigationLink(value: deck.id) {
                                    DeckSummaryRow(deck: deck)
                                }
                                .buttonStyle(.plain)
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

                                if idx < filteredDecks.count - 1 {
                                    Divider().overlay(RemColor.border)
                                }
                            }
                        }
                        .background(RemColor.surface)
                        .clipShape(RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: RemRadius.lg, style: .continuous)
                                .strokeBorder(RemColor.border, lineWidth: 1)
                        )
                    }

                    Button {
                        appState.showNewDeckSheet = true
                    } label: {
                        Label("Create new deck", systemImage: "plus")
                    }
                    .buttonStyle(RemSecondaryButtonStyle())
                }
                .padding(.horizontal, 18)
                .padding(.top, 10)
                .padding(.bottom, 104)
            }
            .background(RemColor.bg)
            .navigationBarHidden(true)
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
}
