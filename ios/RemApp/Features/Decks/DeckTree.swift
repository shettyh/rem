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

    static func isDescendantOrSelf(_ candidate: String, of root: String) -> Bool {
        if candidate == root { return true }
        return candidate.hasPrefix("\(root)\(separator)")
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
            node.deck = summary
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
}
