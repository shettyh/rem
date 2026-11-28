import Foundation

/// Hierarchical representation of Anki-style `Parent::Child` deck names with rolled-up counts.
struct DeckTreeNode: Identifiable {
    let path: String
    let label: String
    let depth: Int
    let deck: DeckSummaryDto?
    let children: [DeckTreeNode]
    let dueCount: UInt32
    let newDueCount: UInt32
    let totalCards: UInt32

    var id: String {
        deck?.id ?? "group:\(path)"
    }

    /// IDs of every real deck in this subtree (self when real + all descendants).
    var subtreeDeckIds: [String] {
        var ids: [String] = []
        if let deck {
            ids.append(deck.id)
        }
        for child in children {
            ids.append(contentsOf: child.subtreeDeckIds)
        }
        return ids
    }

    /// Total number of real decks in this subtree.
    var subtreeDeckCount: Int {
        subtreeDeckIds.count
    }

    /// Number of strict descendant real decks (excluding self when real).
    var subdeckCount: Int {
        max(0, subtreeDeckCount - (deck == nil ? 0 : 1))
    }

    /// Rolled-up total cards across this subtree.
    var subtreeCardCount: UInt32 {
        totalCards
    }
}

struct DeckDeleteCopy: Equatable {
    let title: String
    let message: String
    let confirmLabel: String
}

enum DeckTreeBuilder {
    static let separator = "::"

    static func segments(_ name: String) -> [String] {
        name.components(separatedBy: separator)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    static func leafName(_ name: String) -> String {
        segments(name).last ?? name
    }

    static func ancestorPaths(_ name: String) -> [String] {
        let parts = segments(name)
        guard parts.count > 1 else { return [] }
        return (1..<parts.count).map { len in
            parts[0..<len].joined(separator: separator)
        }
    }

    static func findNode(deckId: String, in roots: [DeckTreeNode]) -> DeckTreeNode? {
        for node in roots {
            if node.deck?.id == deckId {
                return node
            }
            if let found = findNode(deckId: deckId, in: node.children) {
                return found
            }
        }
        return nil
    }

    struct NavigableSubdeck: Identifiable {
        let node: DeckTreeNode
        let deck: DeckSummaryDto
        let displayLabel: String

        var id: String { deck.id }
    }

    /// Promote real decks through any virtual intermediate folders (e.g. synced from older v7 peers)
    /// so every descendant remains reachable from its parent detail screen.
    static func navigableChildren(of parent: DeckTreeNode) -> [NavigableSubdeck] {
        var out: [NavigableSubdeck] = []
        let prefix = "\(parent.path)\(separator)"
        func visit(_ nodes: [DeckTreeNode], viaVirtual: Bool) {
            for child in nodes {
                if let deck = child.deck {
                    let displayLabel: String
                    if viaVirtual && child.path.hasPrefix(prefix) {
                        displayLabel = String(child.path.dropFirst(prefix.count))
                    } else {
                        displayLabel = child.label
                    }
                    out.append(NavigableSubdeck(node: child, deck: deck, displayLabel: displayLabel))
                } else {
                    visit(child.children, viaVirtual: true)
                }
            }
        }
        visit(parent.children, viaVirtual: false)
        return out
    }

    static func isStrictDescendant(_ candidate: String, of root: String) -> Bool {
        candidate.hasPrefix("\(root)\(separator)")
    }

    static func isDescendantOrSelf(_ candidate: String, of root: String) -> Bool {
        if candidate == root { return true }
        return isStrictDescendant(candidate, of: root)
    }

    static func build(from summaries: [DeckSummaryDto]) -> [DeckTreeNode] {
        final class MutableNode {
            let path: String
            let label: String
            let depth: Int
            var deck: DeckSummaryDto?
            var children: [MutableNode] = []

            init(path: String, label: String, depth: Int) {
                self.path = path
                self.label = label
                self.depth = depth
            }
        }

        var roots: [MutableNode] = []
        var byPath: [String: MutableNode] = [:]

        func ensureNode(parts: [String], depth: Int) -> MutableNode {
            let path = parts[0...depth].joined(separator: separator)
            if let existing = byPath[path] { return existing }
            let node = MutableNode(path: path, label: parts[depth], depth: depth)
            byPath[path] = node
            if depth == 0 {
                roots.append(node)
            } else {
                let parent = ensureNode(parts: parts, depth: depth - 1)
                parent.children.append(node)
            }
            return node
        }

        for summary in summaries {
            let parts = segments(summary.name)
            guard !parts.isEmpty else { continue }
            let node = ensureNode(parts: parts, depth: parts.count - 1)
            if node.deck == nil {
                node.deck = summary
            } else {
                // Preserve duplicate-named decks as sibling nodes rather than silently overwriting.
                let dupNode = MutableNode(
                    path: "\(node.path)#\(summary.id)",
                    label: node.label,
                    depth: node.depth
                )
                dupNode.deck = summary
                if node.depth == 0 {
                    roots.append(dupNode)
                } else {
                    let parent = ensureNode(parts: parts, depth: node.depth - 1)
                    parent.children.append(dupNode)
                }
            }
        }

        func freeze(_ node: MutableNode) -> DeckTreeNode {
            let frozenChildren = node.children.map(freeze)
            var due = node.deck?.dueCount ?? 0
            var newDue = node.deck?.newDueCount ?? 0
            var total = node.deck?.totalCards ?? 0
            for child in frozenChildren {
                due += child.dueCount
                newDue += child.newDueCount
                total += child.totalCards
            }
            return DeckTreeNode(
                path: node.path,
                label: node.label,
                depth: node.depth,
                deck: node.deck,
                children: frozenChildren,
                dueCount: due,
                newDueCount: newDue,
                totalCards: total
            )
        }

        return roots.map(freeze)
    }

    static func flatten(_ roots: [DeckTreeNode], collapsed: Set<String>) -> [DeckTreeNode] {
        var out: [DeckTreeNode] = []
        func visit(_ nodes: [DeckTreeNode]) {
            for node in nodes {
                out.append(node)
                if !node.children.isEmpty && !collapsed.contains(node.path) {
                    visit(node.children)
                }
            }
        }
        visit(roots)
        return out
    }

    static func pruneCollapsedPaths(_ collapsed: Set<String>, removing removedPaths: [String]) -> Set<String> {
        guard !removedPaths.isEmpty else { return collapsed }
        return Set(
            collapsed.filter { existing in
                !removedPaths.contains { target in
                    existing == target || isStrictDescendant(existing, of: target)
                }
            }
        )
    }

    static func describeDelete(node: DeckTreeNode) -> DeckDeleteCopy {
        let cards = Int(node.subtreeCardCount)
        let cardsText = "\(cards) \(cards == 1 ? "card" : "cards")"
        if node.deck == nil {
            let subdecks = node.subtreeDeckCount
            let decksText = "\(subdecks) \(subdecks == 1 ? "subdeck" : "subdecks")"
            return DeckDeleteCopy(
                title: "Delete \"\(node.label)\"?",
                message: "Permanently deletes \(decksText) and \(cardsText) inside \"\(node.path)\" locally and on synced devices.",
                confirmLabel: "Delete all"
            )
        }
        let deckName = node.deck?.name ?? node.label
        let subdecks = node.subdeckCount
        if subdecks > 0 {
            let subText = "\(subdecks) \(subdecks == 1 ? "subdeck" : "subdecks")"
            return DeckDeleteCopy(
                title: "Delete \"\(node.label)\" and subdecks?",
                message: "Permanently deletes \"\(deckName)\", \(subText), and \(cardsText) total locally and on synced devices.",
                confirmLabel: "Delete all"
            )
        }
        let body = cards > 0
            ? "Permanently deletes \"\(deckName)\" and its \(cardsText) locally and on synced devices."
            : "Permanently deletes the empty deck \"\(deckName)\" locally and on synced devices."
        return DeckDeleteCopy(
            title: "Delete \"\(node.label)\"?",
            message: body,
            confirmLabel: "Delete"
        )
    }

    static func describeDelete(
        deckId: String,
        deckName: String,
        allSummaries: [DeckSummaryDto]
    ) -> DeckDeleteCopy {
        let direct = allSummaries.first { $0.id == deckId }
        let descendants = allSummaries.filter {
            $0.id != deckId && isStrictDescendant($0.name, of: deckName)
        }
        let label = leafName(deckName)
        let totalCards = Int((direct?.totalCards ?? 0) + descendants.reduce(0) { $0 + $1.totalCards })
        let cardsText = "\(totalCards) \(totalCards == 1 ? "card" : "cards")"
        if !descendants.isEmpty {
            let subText = "\(descendants.count) \(descendants.count == 1 ? "subdeck" : "subdecks")"
            return DeckDeleteCopy(
                title: "Delete \"\(label)\" and subdecks?",
                message: "Permanently deletes \"\(deckName)\", \(subText), and \(cardsText) total locally and on synced devices.",
                confirmLabel: "Delete all"
            )
        }
        let body = totalCards > 0
            ? "Permanently deletes \"\(deckName)\" and its \(cardsText) locally and on synced devices."
            : "Permanently deletes the empty deck \"\(deckName)\" locally and on synced devices."
        return DeckDeleteCopy(
            title: "Delete \"\(label)\"?",
            message: body,
            confirmLabel: "Delete"
        )
    }
}
