import SwiftUI

struct DeckDetailView: View {
    @Bindable var appState: AppState
    let deckId: String

    @Environment(\.dismiss) private var dismiss
    @State private var cards: [CardDto] = []
    @State private var searchText: String = ""
    @State private var selectedTag: String = "All"
    @State private var showDeleteConfirm: Bool = false

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
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Button {
                        dismiss()
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "chevron.left")
                                .font(.system(size: 14, weight: .semibold))
                            Text("Back")
                                .font(.system(size: 15, weight: .medium))
                        }
                        .foregroundStyle(RemColor.muted)
                    }

                    Spacer()

                    Menu {
                        Button("Delete deck", role: .destructive) {
                            showDeleteConfirm = true
                        }
                    } label: {
                        Image(systemName: "ellipsis")
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(RemColor.muted)
                            .frame(width: 32, height: 32)
                    }

                    if let due = deckSummary?.dueCount, due > 0 {
                        Button("Study (\(due))") {
                            appState.startStudy(deckId: deckId)
                        }
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(RemColor.onAccent)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 7)
                        .background(RemColor.accent)
                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    }
                }

                VStack(alignment: .leading, spacing: 4) {
                    Text(deckSummary?.name ?? "Deck")
                        .font(.system(size: 25, weight: .bold))
                        .tracking(-0.5)
                        .foregroundStyle(RemColor.text)

                    if let summary = deckSummary {
                        let cardLabel = "\(cards.count) \(cards.count == 1 ? "card" : "cards")"
                        let newPart = summary.newDueCount > 0 ? " · \(summary.newDueCount) new" : ""
                        let duePart = summary.dueCount > 0 ? " · \(summary.dueCount) due" : (cards.isEmpty ? "" : " · All caught up")
                        Text("\(cardLabel)\(duePart)\(newPart)")
                            .font(.system(size: 13))
                            .foregroundStyle(RemColor.muted)
                    }
                }

                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 14))
                        .foregroundStyle(RemColor.muted)
                    TextField("Filter cards in deck…", text: $searchText)
                        .font(.system(size: 14))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
                .background(RemColor.bgInset)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(RemColor.border, lineWidth: 1)
                )

                if allTags.count > 1 {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 6) {
                            ForEach(allTags, id: \.self) { tag in
                                let active = selectedTag == tag
                                Button {
                                    selectedTag = tag
                                } label: {
                                    Text(tag == "All" ? "All" : "#\(tag)")
                                        .font(.system(size: 12, weight: .medium))
                                        .foregroundStyle(active ? RemColor.bg : RemColor.muted)
                                        .padding(.horizontal, 11)
                                        .padding(.vertical, 6)
                                        .background(active ? RemColor.text : RemColor.surface)
                                        .clipShape(Capsule())
                                        .overlay(
                                            Capsule().strokeBorder(active ? RemColor.text : RemColor.border, lineWidth: 1)
                                        )
                                }
                            }
                        }
                    }
                }

                if filteredCards.isEmpty {
                    RemCardSurface(padding: 20) {
                        VStack(spacing: 6) {
                            Text(cards.isEmpty ? "No cards in this deck" : "No matching cards")
                                .font(.system(size: 15, weight: .semibold))
                                .foregroundStyle(RemColor.text)
                            Text(cards.isEmpty
                                 ? "Tap the + button below to add your first card."
                                 : "Try clearing your search or tag filter.")
                                .font(.system(size: 13))
                                .foregroundStyle(RemColor.muted)
                        }
                        .frame(maxWidth: .infinity)
                    }
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(filteredCards.enumerated()), id: \.element.id) { idx, card in
                            CardBrowserRow(card: card) {
                                appState.openQuickCapture(deckId: deckId, editingCard: card) {
                                    Task { await loadCards() }
                                }
                            }
                            if idx < filteredCards.count - 1 {
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
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
            .padding(.bottom, 104)
        }
        .background(RemColor.bg)
        .navigationBarHidden(true)
        .onAppear {
            appState.activeDeckId = deckId
            Task { await loadCards() }
        }
        .onDisappear {
            if appState.activeDeckId == deckId {
                appState.activeDeckId = nil
            }
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
    }

    private func loadCards() async {
        do {
            self.cards = try await appState.store.listCards(deckId: deckId)
        } catch {
            appState.showToast(error.localizedDescription)
        }
    }
}

struct CardBrowserRow: View {
    let card: CardDto
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 8) {
                Text(card.front)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(card.suspended ? RemColor.muted : RemColor.text)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)

                HStack(spacing: 8) {
                    if let firstTag = card.tags.first {
                        Text("#\(firstTag)")
                            .font(.system(size: 11.5))
                            .foregroundStyle(RemColor.muted)
                    }
                    if !card.assetHashes.isEmpty {
                        Label("\(card.assetHashes.count)", systemImage: "photo")
                            .font(.system(size: 11.5))
                            .foregroundStyle(RemColor.muted)
                    }
                    Spacer()
                    statusPill
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
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
