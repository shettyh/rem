import type { Deck } from '../../domain/models'

export const DECK_SEPARATOR = '::'
export const COLLAPSED_DECKS_EVENT = 'rem:collapsed-decks-changed'

/**
 * Normalize a deck name (including `Parent::Child` nested paths).
 * Rejects blank names and empty `::` segments (such as `::Child`, `Parent::`, or `A::::B`)
 * to stay strictly consistent with Rust `rem_core::deck_tree::normalize_deck_name`.
 */
export function normalizeDeckName(raw: string): string {
  const trimmed = raw.trim()
  if (!trimmed) {
    throw new Error('deck name must not be blank')
  }
  if (!trimmed.includes(DECK_SEPARATOR)) return trimmed
  const segments: string[] = []
  for (const part of trimmed.split(DECK_SEPARATOR)) {
    const seg = part.trim()
    if (!seg) {
      throw new Error("deck name must not contain empty '::' segments")
    }
    segments.push(seg)
  }
  return segments.join(DECK_SEPARATOR)
}

export function deckSegments(name: string): string[] {
  return name
    .split(DECK_SEPARATOR)
    .map((segment) => segment.trim())
    .filter(Boolean)
}

export function deckIsStrictDescendant(candidateName: string, rootName: string): boolean {
  return candidateName.startsWith(`${rootName}${DECK_SEPARATOR}`)
}

export function deckIsDescendantOrSelf(candidateName: string, rootName: string): boolean {
  if (candidateName === rootName) return true
  return deckIsStrictDescendant(candidateName, rootName)
}

export function deckLeafName(name: string): string {
  const parts = deckSegments(name)
  return parts[parts.length - 1] ?? name
}

export interface DeckMetricsItem {
  deck: Deck
  due: number
  newCount?: number
  total?: number
  studied?: number
}

export interface DeckTreeNode<T extends DeckMetricsItem> {
  /** Unique tree node key (full `::`-separated path, or `path#<id>` for duplicate-named decks). */
  path: string
  /** Display label (last path segment). */
  label: string
  /** Nesting depth (0 for top-level). */
  depth: number
  /** Backing real deck item, or null for virtual parent folders. */
  item: T | null
  children: DeckTreeNode<T>[]
  /** Rolled-up counts including self and all descendants. */
  due: number
  newCount: number
  total: number
  studied: number
}

/**
 * Group flat `::`-delimited decks into a collapsible hierarchy with rolled-up counts.
 * Preserves top-level order, synthesizes virtual parent folders when a child deck
 * exists without an explicit parent deck row, and keeps duplicate-named decks visible.
 */
export function buildDeckTree<T extends DeckMetricsItem>(items: T[]): DeckTreeNode<T>[] {
  const roots: DeckTreeNode<T>[] = []
  const byPath = new Map<string, DeckTreeNode<T>>()

  const ensureNode = (segments: string[], depth: number): DeckTreeNode<T> => {
    const path = segments.slice(0, depth + 1).join(DECK_SEPARATOR)
    const existing = byPath.get(path)
    if (existing) return existing
    const node: DeckTreeNode<T> = {
      path,
      label: segments[depth],
      depth,
      item: null,
      children: [],
      due: 0,
      newCount: 0,
      total: 0,
      studied: 0,
    }
    byPath.set(path, node)
    if (depth === 0) {
      roots.push(node)
    } else {
      const parent = ensureNode(segments, depth - 1)
      parent.children.push(node)
    }
    return node
  }

  for (const item of items) {
    const segments = deckSegments(item.deck.name)
    if (segments.length === 0) continue
    const node = ensureNode(segments, segments.length - 1)
    if (node.item === null) {
      node.item = item
    } else {
      // Preserve duplicate-named decks as sibling nodes rather than silently overwriting.
      const dupNode: DeckTreeNode<T> = {
        path: `${node.path}#${item.deck.id}`,
        label: node.label,
        depth: node.depth,
        item,
        children: [],
        due: 0,
        newCount: 0,
        total: 0,
        studied: 0,
      }
      if (node.depth === 0) {
        roots.push(dupNode)
      } else {
        const parent = ensureNode(segments, node.depth - 1)
        parent.children.push(dupNode)
      }
    }
  }

  const rollup = (node: DeckTreeNode<T>): void => {
    let due = node.item?.due ?? 0
    let newCount = node.item?.newCount ?? 0
    let total = node.item?.total ?? 0
    let studied = node.item?.studied ?? 0
    for (const child of node.children) {
      rollup(child)
      due += child.due
      newCount += child.newCount
      total += child.total
      studied += child.studied
    }
    node.due = due
    node.newCount = newCount
    node.total = total
    node.studied = studied
  }

  for (const root of roots) rollup(root)
  return roots
}

export function flattenVisibleDeckTree<T extends DeckMetricsItem>(
  roots: DeckTreeNode<T>[],
  collapsedPaths: Set<string>,
): DeckTreeNode<T>[] {
  const out: DeckTreeNode<T>[] = []
  const visit = (nodes: DeckTreeNode<T>[]) => {
    for (const node of nodes) {
      out.push(node)
      if (node.children.length > 0 && !collapsedPaths.has(node.path)) {
        visit(node.children)
      }
    }
  }
  visit(roots)
  return out
}

export interface DeckSubtreeSummary {
  /** Total real decks in this subtree (including self when backed by a real deck). */
  deckCount: number
  /** Strict descendant real decks (excluding self). */
  subdeckCount: number
  /** Rolled-up total cards across this subtree. */
  cardCount: number
  /** IDs of every real deck in this subtree. */
  deckIds: string[]
  /** Tree node paths in this subtree (used to prune collapsed state). */
  paths: string[]
}

export function summarizeDeckSubtree<T extends DeckMetricsItem>(
  node: DeckTreeNode<T>,
): DeckSubtreeSummary {
  const deckIds: string[] = []
  const paths: string[] = []
  const visit = (current: DeckTreeNode<T>) => {
    paths.push(current.path)
    if (current.item) deckIds.push(current.item.deck.id)
    for (const child of current.children) visit(child)
  }
  visit(node)
  const deckCount = deckIds.length
  const subdeckCount = Math.max(0, deckCount - (node.item ? 1 : 0))
  return {
    deckCount,
    subdeckCount,
    cardCount: node.total,
    deckIds,
    paths,
  }
}

export interface DeckDeleteCopy {
  title: string
  description: string
  confirmLabel: string
}

export function describeDeckDelete<T extends DeckMetricsItem>(
  node: DeckTreeNode<T>,
): DeckDeleteCopy {
  const summary = summarizeDeckSubtree(node)
  const cardsText = `${summary.cardCount} ${summary.cardCount === 1 ? 'card' : 'cards'}`
  if (!node.item) {
    const decksText = `${summary.deckCount} ${summary.deckCount === 1 ? 'subdeck' : 'subdecks'}`
    return {
      title: `Delete "${node.label}"?`,
      description: `Permanently deletes ${decksText} and ${cardsText} inside "${node.path}". This can't be undone.`,
      confirmLabel: 'Delete all',
    }
  }
  if (summary.subdeckCount > 0) {
    const subText = `${summary.subdeckCount} ${summary.subdeckCount === 1 ? 'subdeck' : 'subdecks'}`
    return {
      title: `Delete "${node.label}" and subdecks?`,
      description: `Permanently deletes "${node.item.deck.name}", ${subText}, and ${cardsText} total. This can't be undone.`,
      confirmLabel: 'Delete all',
    }
  }
  return {
    title: `Delete "${node.label}"?`,
    description:
      summary.cardCount > 0
        ? `Permanently deletes "${node.item.deck.name}" and its ${cardsText}. This can't be undone.`
        : `Permanently deletes the empty deck "${node.item.deck.name}". This can't be undone.`,
    confirmLabel: 'Delete deck',
  }
}

export function pruneCollapsedDeckPaths(
  collapsedPaths: Set<string>,
  removedPaths: Iterable<string>,
): Set<string> {
  const removed = [...removedPaths]
  if (removed.length === 0) return new Set(collapsedPaths)
  const next = new Set<string>()
  for (const existing of collapsedPaths) {
    const matchesRemoved = removed.some(
      (target) => existing === target || deckIsStrictDescendant(existing, target),
    )
    if (!matchesRemoved) next.add(existing)
  }
  return next
}

const COLLAPSED_DECKS_STORAGE_KEY = 'rem:collapsed-deck-paths'

export function loadCollapsedDeckPaths(): Set<string> {
  try {
    const raw = window.localStorage.getItem(COLLAPSED_DECKS_STORAGE_KEY)
    if (!raw) return new Set()
    const parsed = JSON.parse(raw)
    if (Array.isArray(parsed)) {
      return new Set(parsed.filter((v): v is string => typeof v === 'string'))
    }
  } catch {
    // ignore storage errors
  }
  return new Set()
}

export function saveCollapsedDeckPaths(paths: Set<string>): void {
  try {
    window.localStorage.setItem(COLLAPSED_DECKS_STORAGE_KEY, JSON.stringify([...paths]))
    window.dispatchEvent(new CustomEvent(COLLAPSED_DECKS_EVENT))
  } catch {
    // ignore storage errors
  }
}
