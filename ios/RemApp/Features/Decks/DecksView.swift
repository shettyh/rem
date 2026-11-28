import SwiftUI

struct DecksView: View {
    private enum PendingDeckDelete {
        case deck(id: String, copy: DeckDeleteCopy)
        case tree(path: String, copy: DeckDeleteCopy)

        var copy: DeckDeleteCopy {
            switch self {
            case .deck(_, let copy), .tree(_, let copy):
                return copy
            }
        }
    }

    @Bindable var appState: AppState
    @State private var navPath: [String] = []
    @State private var searchText: String = ""
    @State private var cardEntries: [CardSearchEntry] = []
    @State private var settledSearchQuery: String = ""
    @State private var isSearching: Bool = false
    @State private var searchError: String? = nil
    @State private var searchToken: Int = 0
    @State private var pendingDelete: PendingDeckDelete?
    @State private var showImportDialog: Bool = false

    private var trimmedSearch: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var isSearchPending: Bool {
        !trimmedSearch.isEmpty && (isSearching || settledSearchQuery != trimmedSearch)
    }

    private var allDeckSummaries: [DeckSummaryDto] {
        appState.todaySummary?.decks ?? []
    }

    private var filteredDecks: [DeckSummaryDto] {
        guard !trimmedSearch.isEmpty else { return allDeckSummaries }
        return allDeckSummaries.filter {
            CardSearchResultMeta.matchesDeck(name: $0.name, query: trimmedSearch)
        }
    }

    private func rolledUpTotals(for deck: DeckSummaryDto) -> (due: UInt32, newDue: UInt32, total: UInt32) {
        let descendants = allDeckSummaries.filter {
            $0.id == deck.id || DeckTreeBuilder.isStrictDescendant($0.name, of: deck.name)
        }
        return (
            due: descendants.reduce(0) { $0 + $1.dueCount },
            newDue: descendants.reduce(0) { $0 + $1.newDueCount },
            total: descendants.reduce(0) { $0 + $1.totalCards }
        )
    }

    private var deckTreeRoots: [DeckTreeNode] {
        DeckTreeBuilder.build(from: allDeckSummaries)
    }

    private var hasAnyNestedDecks: Bool {
        deckTreeRoots.contains { !$0.children.isEmpty }
    }

    private var treeNodes: [DeckTreeNode] {
        DeckTreeBuilder.flatten(
            deckTreeRoots,
            collapsed: appState.collapsedDeckPaths
        )
    }

    private func toggleCollapse(_ path: String) {
        withAnimation(.easeOut(duration: 0.16)) {
            appState.toggleDeckCollapse(path)
        }
    }

    var body: some View {
        NavigationStack(path: $navPath) {
            Group {
                if filteredDecks.isEmpty && cardEntries.isEmpty && !trimmedSearch.isEmpty && !isSearchPending && searchError == nil {
                    ContentUnavailableView.search(text: trimmedSearch)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(RemColor.groupedBg)
                } else if filteredDecks.isEmpty && trimmedSearch.isEmpty {
                    ContentUnavailableView {
                        Label("No decks yet", systemImage: "rectangle.stack")
                    } description: {
                        Text("Create a deck or import from Anki (.apkg) or a rem backup (.json).")
                    } actions: {
                        Button("Create new deck") {
                            appState.showNewDeckSheet = true
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(RemColor.accent)

                        Button {
                            showImportDialog = true
                        } label: {
                            Text(appState.isImporting ? "Importing…" : "Import deck")
                        }
                        .buttonStyle(.bordered)
                        .tint(RemColor.accent)
                        .disabled(!appState.canRequestDeckImport)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(RemColor.groupedBg)
                } else {
                    List {
                        if trimmedSearch.isEmpty && !treeNodes.isEmpty {
                            let showTreeGutter = hasAnyNestedDecks
                            Section {
                                ForEach(treeNodes) { node in
                                    if let deck = node.deck {
                                        let isCollapsed = appState.collapsedDeckPaths.contains(node.path)
                                        let deleteCopy = DeckTreeBuilder.describeDelete(node: node)
                                        HStack(spacing: 4) {
                                            if showTreeGutter {
                                                if !node.children.isEmpty {
                                                    Button {
                                                        toggleCollapse(node.path)
                                                    } label: {
                                                        Image(systemName: isCollapsed ? "chevron.right" : "chevron.down")
                                                            .font(.system(size: 11, weight: .semibold))
                                                            .foregroundStyle(RemColor.faint)
                                                            .frame(width: 24, height: 32)
                                                            .contentShape(Rectangle())
                                                    }
                                                    .buttonStyle(.plain)
                                                    .accessibilityLabel(isCollapsed ? "Expand \(node.label)" : "Collapse \(node.label)")
                                                } else {
                                                    Color.clear
                                                        .frame(width: 24, height: 1)
                                                }
                                            }
                                            NavigationLink(value: deck.id) {
                                                DeckSummaryRow(
                                                    deck: deck,
                                                    showChevron: false,
                                                    displayName: node.label,
                                                    depth: 0,
                                                    hasSubdecks: !node.children.isEmpty,
                                                    subdeckCount: node.subdeckCount,
                                                    dueOverride: node.dueCount,
                                                    newDueOverride: node.newDueCount,
                                                    totalOverride: node.totalCards,
                                                    iconSlotWidth: showTreeGutter ? 16 : nil
                                                )
                                            }
                                        }
                                        .padding(.leading, CGFloat(min(node.depth, 4)) * 16)
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
                                                pendingDelete = .deck(id: deck.id, copy: deleteCopy)
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
                                                pendingDelete = .deck(id: deck.id, copy: deleteCopy)
                                            } label: {
                                                Label(
                                                    node.children.isEmpty ? "Delete deck" : "Delete deck & subdecks",
                                                    systemImage: "trash"
                                                )
                                            }
                                        }
                                    } else {
                                        let deleteCopy = DeckTreeBuilder.describeDelete(node: node)
                                        VirtualDeckFolderRow(
                                            node: node,
                                            isCollapsed: appState.collapsedDeckPaths.contains(node.path),
                                            insetPadding: false
                                        ) {
                                            toggleCollapse(node.path)
                                        }
                                        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                                            Button(role: .destructive) {
                                                pendingDelete = .tree(path: node.path, copy: deleteCopy)
                                            } label: {
                                                Label("Delete", systemImage: "trash")
                                            }
                                        }
                                        .contextMenu {
                                            Button(role: .destructive) {
                                                pendingDelete = .tree(path: node.path, copy: deleteCopy)
                                            } label: {
                                                Label("Delete folder", systemImage: "trash")
                                            }
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
                                    let matchedNode = DeckTreeBuilder.findNode(deckId: deck.id, in: deckTreeRoots)
                                    let deleteCopy = DeckTreeBuilder.describeDelete(
                                        deckId: deck.id,
                                        deckName: deck.name,
                                        allSummaries: allDeckSummaries
                                    )
                                    NavigationLink(value: deck.id) {
                                        DeckSummaryRow(
                                            deck: deck,
                                            showChevron: false,
                                            displayName: CardSearchResultMeta.formatDeckPath(deck.name),
                                            hasSubdecks: !(matchedNode?.children.isEmpty ?? true),
                                            subdeckCount: matchedNode?.subdeckCount ?? 0,
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
                                            pendingDelete = .deck(id: deck.id, copy: deleteCopy)
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
                                            pendingDelete = .deck(id: deck.id, copy: deleteCopy)
                                        } label: {
                                            Label(
                                                allDeckSummaries.contains {
                                                    DeckTreeBuilder.isStrictDescendant($0.name, of: deck.name)
                                                } ? "Delete deck & subdecks" : "Delete deck",
                                                systemImage: "trash"
                                            )
                                        }
                                    }
                                }
                            }
                            .listRowBackground(RemColor.surface)
                            .listRowSeparatorTint(RemColor.border)
                        }

                        if !trimmedSearch.isEmpty && (isSearchPending || searchError != nil || !cardEntries.isEmpty) {
                            Section("Cards") {
                                if isSearchPending && cardEntries.isEmpty {
                                    HStack(spacing: 10) {
                                        ProgressView()
                                            .controlSize(.small)
                                        Text("Searching cards…")
                                            .font(.subheadline)
                                            .foregroundStyle(RemColor.muted)
                                    }
                                    .padding(.vertical, 4)
                                } else if let error = searchError, cardEntries.isEmpty {
                                    HStack(spacing: 12) {
                                        Text(error)
                                            .font(.subheadline)
                                            .foregroundStyle(RemColor.muted)
                                        Spacer(minLength: 8)
                                        Button("Retry") {
                                            Task { await runCardSearch(trimmedSearch) }
                                        }
                                        .font(.subheadline.weight(.medium))
                                        .tint(RemColor.accent)
                                    }
                                    .padding(.vertical, 4)
                                } else {
                                    ForEach(cardEntries) { entry in
                                        CardSearchResultRow(entry: entry) {
                                            appState.openQuickCapture(
                                                deckId: entry.hit.card.deckId,
                                                editingCard: entry.hit.card
                                            ) {
                                                Task { await runCardSearch(trimmedSearch) }
                                            }
                                        }
                                        .opacity(isSearchPending ? 0.55 : 1)
                                        .disabled(isSearchPending)
                                    }
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

                                Button {
                                    showImportDialog = true
                                } label: {
                                    HStack(spacing: 10) {
                                        Image(systemName: "square.and.arrow.down")
                                            .foregroundStyle(appState.isImporting ? RemColor.muted : RemColor.accent)
                                        Text(appState.isImporting ? "Importing deck…" : "Import from Anki or JSON")
                                            .font(.body.weight(.medium))
                                            .foregroundStyle(appState.isImporting ? RemColor.muted : RemColor.text)
                                        Spacer()
                                        if appState.isImporting {
                                            ProgressView().controlSize(.small)
                                        }
                                    }
                                    .contentShape(Rectangle())
                                }
                                .disabled(!appState.canRequestDeckImport)
                            }
                            .listRowBackground(RemColor.surface)
                            .listRowSeparatorTint(RemColor.border)
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
            .onAppear {
                if !trimmedSearch.isEmpty {
                    Task { await runCardSearch(trimmedSearch) }
                }
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
                DeckDetailView(appState: appState, deckId: deckId) { targetId in
                    if let existingIndex = navPath.firstIndex(of: targetId) {
                        navPath.removeSubrange((existingIndex + 1)..<navPath.count)
                    } else {
                        navPath.append(targetId)
                    }
                }
            }
            .alert(
                pendingDelete?.copy.title ?? "Delete deck?",
                isPresented: Binding(
                    get: { pendingDelete != nil },
                    set: { if !$0 { pendingDelete = nil } }
                )
            ) {
                if let target = pendingDelete {
                    Button(target.copy.confirmLabel, role: .destructive) {
                        pendingDelete = nil
                        Task {
                            switch target {
                            case .deck(let id, _):
                                await appState.deleteDeck(id: id)
                            case .tree(let path, _):
                                await appState.deleteDeckTree(path: path)
                            }
                            if !trimmedSearch.isEmpty {
                                await runCardSearch(trimmedSearch)
                            }
                        }
                    }
                }
                Button("Cancel", role: .cancel) {
                    pendingDelete = nil
                }
            } message: {
                Text(pendingDelete?.copy.message ?? "This deletes the deck and all its cards locally and on synced devices.")
            }
            .confirmationDialog(
                "Import deck",
                isPresented: $showImportDialog,
                titleVisibility: .visible
            ) {
                Button("Anki deck (.apkg)") {
                    appState.requestDeckImport(.apkg, afterDelayMs: 150)
                }
                Button("rem backup (.json)") {
                    appState.requestDeckImport(.backup, afterDelayMs: 150)
                }
                Button("Cancel", role: .cancel) {}
            }
        }
    }

    private func runCardSearch(_ query: String, debounce: Bool = false) async {
        searchToken += 1
        let token = searchToken
        guard !query.isEmpty else {
            isSearching = false
            searchError = nil
            settledSearchQuery = ""
            cardEntries = []
            return
        }
        isSearching = true
        searchError = nil
        defer {
            if !Task.isCancelled && token == searchToken {
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
            cardEntries = results.map { hit in
                CardSearchEntry(hit: hit, meta: CardSearchResultMeta.build(hit: hit, query: query))
            }
            settledSearchQuery = query
            searchError = nil
        } catch {
            guard !Task.isCancelled && token == searchToken else { return }
            cardEntries = []
            settledSearchQuery = query
            searchError = "Couldn't search cards."
        }
    }
}

struct CardSearchEntry: Identifiable {
    let hit: CardSearchHitDto
    let meta: CardSearchResultMeta
    var id: String { hit.card.id }
}

struct CardSearchResultMeta: Equatable {
    let displayDeckName: String
    let frontPreview: String
    let backPreview: String?
    let clozeBadge: String?
    let rationalePreview: String?
    let visibleTags: [String]
    let matchingTags: Set<String>

    static func formatDeckPath(_ name: String) -> String {
        name.replacingOccurrences(of: DeckTreeBuilder.separator, with: " / ")
    }

    static func matchesDeck(name: String, query: String) -> Bool {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        let lower = trimmed.lowercased()
        let rawLower = name.lowercased()
        let formattedLower = formatDeckPath(name).lowercased()
        if rawLower.contains(lower) || formattedLower.contains(lower) {
            return true
        }
        if hasCodePunctuation(trimmed) {
            return false
        }
        let words = trimmed
            .components(separatedBy: .whitespacesAndNewlines)
            .map { $0.trimmingCharacters(in: CharacterSet(charactersIn: "\"'")).lowercased() }
            .filter { !$0.isEmpty }
        return words.count > 1 && words.allSatisfy { formattedLower.contains($0) }
    }

    static func build(hit: CardSearchHitDto, query: String) -> CardSearchResultMeta {
        let card = hit.card
        let frontPlain = MarkdownBlockParser.plainTextPreview(card.front)
        let frontPreview = frontPlain.isEmpty ? card.front.trimmingCharacters(in: .whitespacesAndNewlines) : frontPlain

        let backPlain = MarkdownBlockParser.plainTextPreview(card.back)
        let backTrimmed = backPlain.isEmpty ? card.back.trimmingCharacters(in: .whitespacesAndNewlines) : backPlain
        let backPreview = backTrimmed.isEmpty ? nil : backTrimmed

        let clozeBadge: String? = card.clozeIndex.flatMap { idx in
            guard idx > 0 else { return nil }
            return ImageOcclusionParser.hasOcclusion(card.front) ? "IO \(idx)" : "c\(idx)"
        }

        let terms = searchTerms(from: query)
        let matchingTagList = card.tags.filter { matchesAny($0, terms: terms) }
        let matchingTags = Set(matchingTagList)
        let orderedTags = matchingTagList + card.tags.filter { !matchingTags.contains($0) }
        let visibleTags = Array(orderedTags.prefix(3))

        let matchedPrimary = matchesAny(frontPreview, terms: terms)
            || matchesAny(card.front, terms: terms)
            || (backPreview.map { matchesAny($0, terms: terms) } ?? false)
            || matchesAny(card.back, terms: terms)
            || !matchingTags.isEmpty

        let rationalePlain = card.rationale.map { MarkdownBlockParser.plainTextPreview($0) } ?? ""
        let rationaleTrimmed = rationalePlain.isEmpty
            ? (card.rationale?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "")
            : rationalePlain
        let rationalePreview: String? = (!matchedPrimary && !rationaleTrimmed.isEmpty && matchesAny(rationaleTrimmed, terms: terms))
            ? rationaleTrimmed
            : nil

        return CardSearchResultMeta(
            displayDeckName: formatDeckPath(hit.deckName),
            frontPreview: frontPreview.isEmpty ? "(empty front)" : frontPreview,
            backPreview: backPreview,
            clozeBadge: clozeBadge,
            rationalePreview: rationalePreview,
            visibleTags: visibleTags,
            matchingTags: matchingTags
        )
    }

    static func matchesInDeckFilter(card: CardDto, query: String) -> Bool {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        let haystack = [
            card.front,
            card.back,
            card.tags.joined(separator: " "),
            card.rationale ?? ""
        ].joined(separator: "\n").lowercased()

        let lower = trimmed.lowercased()
        if haystack.contains(lower) { return true }
        if hasCodePunctuation(trimmed) { return false }

        let words = trimmed
            .components(separatedBy: .whitespacesAndNewlines)
            .map { $0.trimmingCharacters(in: CharacterSet(charactersIn: "\"'")).lowercased() }
            .filter { !$0.isEmpty }
        return words.count > 1 && words.allSatisfy { haystack.contains($0) }
    }

    static func rationaleOnlyPreview(card: CardDto, query: String) -> String? {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let rawRationale = card.rationale, !rawRationale.isEmpty else {
            return nil
        }
        let terms = searchTerms(from: trimmed)
        let matchedPrimary = matchesAny(card.front, terms: terms)
            || matchesAny(card.back, terms: terms)
            || card.tags.contains { matchesAny($0, terms: terms) }
        guard !matchedPrimary else { return nil }
        let plain = MarkdownBlockParser.plainTextPreview(rawRationale)
        let resolved = plain.isEmpty ? rawRationale.trimmingCharacters(in: .whitespacesAndNewlines) : plain
        return matchesAny(resolved, terms: terms) ? resolved : nil
    }

    private static func hasCodePunctuation(_ query: String) -> Bool {
        let allowed = CharacterSet.alphanumerics
            .union(.whitespacesAndNewlines)
            .union(CharacterSet(charactersIn: "-'\""))
        return query.unicodeScalars.contains { !allowed.contains($0) }
    }

    private static func searchTerms(from query: String) -> [String] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }
        if hasCodePunctuation(trimmed) {
            return [trimmed.lowercased()]
        }
        let rawTokens = trimmed
            .components(separatedBy: .whitespacesAndNewlines)
            .map { $0.trimmingCharacters(in: CharacterSet(charactersIn: "\"'")).lowercased() }
            .filter { !$0.isEmpty }
        var expanded: [String] = []
        for token in rawTokens {
            expanded.append(token)
            if token.contains("-") {
                for part in token.split(separator: "-") where !part.isEmpty {
                    expanded.append(String(part))
                }
            }
        }
        return expanded.isEmpty ? [trimmed.lowercased()] : Array(Set(expanded))
    }

    private static func matchesAny(_ text: String, terms: [String]) -> Bool {
        guard !text.isEmpty, !terms.isEmpty else { return false }
        let lower = text.lowercased()
        return terms.contains { lower.contains($0) }
    }
}

private struct CardSearchResultRow: View {
    let entry: CardSearchEntry
    let onTap: () -> Void

    var body: some View {
        let hit = entry.hit
        let meta = entry.meta
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Circle()
                        .fill(Color(hexString: hit.deckColor))
                        .frame(width: 7, height: 7)
                    Text(meta.displayDeckName)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(RemColor.muted)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    if meta.rationalePreview != nil {
                        RemPillBadge(text: "Rationale", tone: .neutral)
                    }
                    if hit.card.suspended {
                        RemPillBadge(text: "Suspended", tone: .neutral)
                    }
                }

                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    if let badge = meta.clozeBadge {
                        Text(badge)
                            .font(.caption2.weight(.bold).monospaced())
                            .foregroundStyle(RemColor.accent)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(RemColor.accentTint)
                            .clipShape(Capsule())
                    }
                    Text(meta.frontPreview)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(hit.card.suspended ? RemColor.muted : RemColor.text)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                }

                if let back = meta.backPreview {
                    Text(back)
                        .font(.footnote)
                        .foregroundStyle(RemColor.muted)
                        .lineLimit(1)
                }

                if let rationale = meta.rationalePreview {
                    Text(rationale)
                        .font(.caption.italic())
                        .foregroundStyle(RemColor.muted)
                        .lineLimit(1)
                }

                if !meta.visibleTags.isEmpty {
                    HStack(spacing: 8) {
                        ForEach(meta.visibleTags, id: \.self) { tag in
                            Text("#\(tag)")
                                .font(.caption)
                                .foregroundStyle(
                                    meta.matchingTags.contains(tag) ? RemColor.accent : RemColor.faint
                                )
                        }
                    }
                    .padding(.top, 1)
                }
            }
            .padding(.vertical, 3)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
