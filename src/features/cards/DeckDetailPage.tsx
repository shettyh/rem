import { useEffect, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import { MS_PER_DAY, type Card } from '../../domain/models'
import { EmptyIcon } from '../../ui/EmptyIcon'
import { PageHeader } from '../../ui/PageHeader'
import { isNew, loadDueOverview, type DeckOverview } from '../review/dueOverview'
import {
  buildDeckTree,
  deckAncestorPaths,
  deckLeafName,
  summarizeDeckSubtree,
  type DeckTreeNode,
} from '../decks/deckTree'
import { userTags } from './cardTags'
import { stripClozeMarkup } from './cloze'
import { hasImageOcclusion, stripOcclusionMarkup } from './occlusion'
import { stripRubyMarkup } from './ruby'

/** Clean a single line of markdown into plain preview text. */
function cleanMarkdownLine(line: string): string {
  return stripRubyMarkup(stripClozeMarkup(line))
    .replace(/^#{1,6}\s+/, '') // heading
    .replace(/^>\s?/, '') // blockquote
    .replace(/^[-*+]\s+/, '') // bullet list
    .replace(/^\d+\.\s+/, '') // ordered list
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, (_m, alt: string) => alt.trim() || '[image]') // image -> alt or placeholder
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1') // link / audio -> label
    .replace(/(\*\*|__|~~|[*_`])/g, '') // emphasis / inline-code markers
    .trim()
}

/** First non-empty line of markdown, reduced to plain text; skips fenced code openers. */
export function cardPreview(md: string): string {
  return cardSnippet(md).text
}

/** Structured one-line preview that preserves code-block language metadata instead of showing "rust". */
export function cardSnippet(md: string): { text: string; lang: string | null } {
  const normalized = hasImageOcclusion(md) ? stripOcclusionMarkup(md) : md
  const lines = normalized.split('\n')
  const firstIdx = lines.findIndex((l) => l.trim().length > 0)
  if (firstIdx === -1) return { text: '', lang: null }

  const first = lines[firstIdx].trim()
  const fence = first.match(/^(`{3,}|~{3,})\s*([a-zA-Z0-9_+-]*)\s*$/)
  if (fence) {
    const marker = fence[1][0]
    const lang = fence[2]?.trim() || null
    for (let i = firstIdx + 1; i < lines.length; i++) {
      const candidate = lines[i].trim()
      if (!candidate) continue
      if (candidate.startsWith(marker.repeat(3))) break
      return { text: candidate, lang }
    }
    return { text: '', lang }
  }

  return { text: cleanMarkdownLine(first), lang: null }
}

/** A card's management status; suspension/leech metadata takes priority over scheduling. */
export function cardStatus(
  card: Pick<Card, 'scheduling' | 'tags' | 'suspended'>,
  now: number,
): { kind: 'new' | 'due' | 'scheduled' | 'leech' | 'suspended'; label: string } {
  if (card.suspended) return { kind: 'suspended', label: 'suspended' }
  if (card.tags.includes('leech')) return { kind: 'leech', label: 'leech' }
  const s = card.scheduling
  const isNewCard = s.reps === 0
  if (isNewCard) return { kind: 'new', label: 'new' }
  if (s.due <= now) return { kind: 'due', label: 'due' }
  const days = Math.max(1, Math.round((s.due - now) / MS_PER_DAY))
  const label = days >= 30 ? `${Math.round(days / 30)}mo` : `${days}d`
  return { kind: 'scheduled', label }
}

function findTreeNodeByDeckId(
  nodes: DeckTreeNode<DeckOverview>[],
  deckId: string,
): DeckTreeNode<DeckOverview> | null {
  for (const node of nodes) {
    if (node.item?.deck.id === deckId) return node
    const nested = findTreeNodeByDeckId(node.children, deckId)
    if (nested) return nested
  }
  return null
}

interface NavigableSubdeck {
  node: DeckTreeNode<DeckOverview>
  deck: DeckOverview['deck']
  displayLabel: string
}

/** Promote real decks through any virtual intermediate folders (e.g. from older v7 sync peers)
 *  so every descendant remains reachable from its parent page. */
function collectNavigableSubdecks(
  children: DeckTreeNode<DeckOverview>[],
  parentName: string,
): NavigableSubdeck[] {
  const out: NavigableSubdeck[] = []
  const prefix = `${parentName}::`
  const visit = (nodes: DeckTreeNode<DeckOverview>[], viaVirtual: boolean) => {
    for (const child of nodes) {
      if (child.item) {
        const displayLabel =
          viaVirtual && child.path.startsWith(prefix) ? child.path.slice(prefix.length) : child.label
        out.push({ node: child, deck: child.item.deck, displayLabel })
      } else {
        visit(child.children, true)
      }
    }
  }
  visit(children, false)
  return out
}

type StatusFilter = 'all' | 'due' | 'new' | 'suspended'

export function DeckDetailPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
  const searchRef = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all')
  const [tagFilter, setTagFilter] = useState('')
  const [addingSubdeck, setAddingSubdeck] = useState(false)
  const [subdeckName, setSubdeckName] = useState('')
  const [subdeckError, setSubdeckError] = useState<string | null>(null)

  useEffect(() => {
    setQuery('')
    setStatusFilter('all')
    setTagFilter('')
    setAddingSubdeck(false)
    setSubdeckName('')
    setSubdeckError(null)
  }, [deckId])

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const cards = useStorageQuery(() => (deckId ? storage.listCards(deckId) : []), [deckId])
  const subdecksInfo = useStorageQuery(async () => {
    if (!deckId) {
      return {
        totalDue: 0,
        rolledNew: 0,
        rolledTotal: 0,
        rolledStudied: 0,
        subdeckCount: 0,
        ancestors: [] as { path: string; label: string; deckId: string | null }[],
        directSubdecks: [] as NavigableSubdeck[],
      }
    }
    const overview = await loadDueOverview(storage, Date.now())
    const current = overview.decks.find((entry) => entry.deck.id === deckId)
    if (!current) {
      return {
        totalDue: 0,
        rolledNew: 0,
        rolledTotal: 0,
        rolledStudied: 0,
        subdeckCount: 0,
        ancestors: [],
        directSubdecks: [],
      }
    }
    const byName = new Map<string, string>()
    for (const entry of overview.decks) {
      if (!byName.has(entry.deck.name)) byName.set(entry.deck.name, entry.deck.id)
    }
    const ancestors = deckAncestorPaths(current.deck.name).map((path) => ({
      path,
      label: deckLeafName(path),
      deckId: byName.get(path) ?? null,
    }))
    const tree = buildDeckTree(overview.decks)
    const node = findTreeNodeByDeckId(tree, deckId)
    if (!node) {
      return {
        totalDue: current.due,
        rolledNew: Math.max(0, current.total - current.studied),
        rolledTotal: current.total,
        rolledStudied: current.studied,
        subdeckCount: 0,
        ancestors,
        directSubdecks: [],
      }
    }
    const summary = summarizeDeckSubtree(node)
    return {
      totalDue: node.due,
      rolledNew: Math.max(0, node.total - node.studied),
      rolledTotal: node.total,
      rolledStudied: node.studied,
      subdeckCount: summary.subdeckCount,
      ancestors,
      directSubdecks: collectNavigableSubdecks(node.children, current.deck.name),
    }
  }, [deckId])
  const due = subdecksInfo?.totalDue ?? 0
  const directSubdecks = subdecksInfo?.directSubdecks ?? []
  const subdeckCount = subdecksInfo?.subdeckCount ?? 0
  const ancestors = subdecksInfo?.ancestors ?? []

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement | null)?.tagName ?? ''
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
      if (e.key === '/' && searchRef.current) {
        e.preventDefault()
        searchRef.current.focus()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  if (!deckId || deck === undefined || cards === undefined || subdecksInfo === undefined) return null

  const now = Date.now()
  const directNewCount = cards.filter((c) => isNew(c.scheduling)).length
  const totalCount = subdeckCount > 0 ? (subdecksInfo?.rolledTotal ?? cards.length) : cards.length
  const newCount = subdeckCount > 0 ? (subdecksInfo?.rolledNew ?? directNewCount) : directNewCount
  const studiedCount =
    subdeckCount > 0
      ? (subdecksInfo?.rolledStudied ?? (cards.length - directNewCount))
      : cards.length - directNewCount
  const studiedPct = totalCount > 0 ? Math.round((studiedCount / totalCount) * 100) : 0
  const tagOptions = [...new Set(cards.flatMap((card) => userTags(card.tags)))]
    .sort((a, b) => a.localeCompare(b))
  const needle = query.trim().toLowerCase()

  const visibleCards = cards.filter((card) => {
    if (tagFilter && !card.tags.includes(tagFilter)) return false
    if (statusFilter !== 'all') {
      const status = cardStatus(card, now)
      if (statusFilter === 'due' && status.kind !== 'due' && status.kind !== 'new') return false
      if (statusFilter === 'new' && !isNew(card.scheduling)) return false
      if (statusFilter === 'suspended' && !card.suspended) return false
    }
    if (needle) {
      const hay = `${card.front}\n${card.back}\n${card.tags.join(' ')}`.toLowerCase()
      if (!hay.includes(needle)) return false
    }
    return true
  })

  const hasFilters = Boolean(needle || tagFilter || statusFilter !== 'all')

  async function toggleSuspend(card: Card) {
    await storage.updateCard(card.id, { suspended: !card.suspended })
  }

  async function createSubdeck(e: React.FormEvent) {
    e.preventDefault()
    const childLeaf = subdeckName.trim()
    if (!childLeaf || !deck) return
    try {
      await storage.createDeck(`${deck.name}::${childLeaf}`)
      setSubdeckName('')
      setSubdeckError(null)
      setAddingSubdeck(false)
    } catch (err) {
      setSubdeckError(err instanceof Error ? err.message : 'Invalid subdeck name.')
    }
  }

  const leafLabel = deckLeafName(deck.name)
  const hasAnyCards = cards.length > 0 || totalCount > 0
  const title = (
    <>
      {ancestors.length > 0 ? (
        <span className="header-breadcrumbs" aria-label="Deck path">
          {ancestors.map((crumb) => (
            <span key={crumb.path} className="header-crumb-item">
              {crumb.deckId ? (
                <Link to={`/decks/${crumb.deckId}`} className="header-crumb-link" title={crumb.path}>
                  {crumb.label}
                </Link>
              ) : (
                <span className="header-crumb-text" title={crumb.path}>{crumb.label}</span>
              )}
              <span className="header-crumb-sep" aria-hidden="true">/</span>
            </span>
          ))}
          <span className="header-title-text" title={deck.name}>{leafLabel}</span>
        </span>
      ) : (
        <span className="header-title-text" title={deck.name}>{deck.name}</span>
      )}
      {hasAnyCards && (
        due > 0 ? (
          <span className="header-status-pill is-due">
            <span className="status-dot" aria-hidden="true" />
            {due} due
          </span>
        ) : (
          <span className="header-status-pill is-good">
            <span className="status-dot" aria-hidden="true" />
            Caught up
          </span>
        )
      )}
    </>
  )

  const actions = (
    <>
      <button
        type="button"
        className="btn btn-ghost btn-icon"
        aria-label="Options"
        title="Deck options"
        onClick={() => navigate(`/decks/${deckId}/options`)}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M2.5 4.5h6M11.5 4.5h2M2.5 8h2M7.5 8h6M2.5 11.5h7M12.5 11.5h1" />
          <circle cx="10" cy="4.5" r="1.4" />
          <circle cx="6" cy="8" r="1.4" />
          <circle cx="11" cy="11.5" r="1.4" />
        </svg>
      </button>
      {hasAnyCards ? (
        due > 0 ? (
          <>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => navigate(`/decks/${deckId}/cards/new`)}
            >
              + Add card
            </button>
            <Link to={`/decks/${deckId}/study`} className="btn btn-primary">
              Study {due}
            </Link>
          </>
        ) : (
          <>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => navigate(`/decks/${deckId}/options#custom-study`)}
            >
              Custom study
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => navigate(`/decks/${deckId}/cards/new`)}
            >
              + Add card
            </button>
          </>
        )
      ) : subdeckCount > 0 ? (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => navigate(`/decks/${deckId}/cards/new`)}
        >
          + Add card
        </button>
      ) : null}
    </>
  )

  const statusTabs: { value: StatusFilter; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'due', label: 'Due' },
    { value: 'new', label: 'New' },
    { value: 'suspended', label: 'Suspended' },
  ]

  return (
    <>
      <PageHeader title={title} actions={actions} />
      <div className="page-body">
        {(cards.length > 0 || (subdeckCount > 0 && totalCount > 0)) && (
          <section className="deck-summary-bar" aria-label="Deck overview">
            <div className="deck-stats" aria-label="Deck summary">
              <div className="deck-stat deck-stat-due">
                <span className="deck-stat-num">{due ?? 0}</span>
                <span className="deck-stat-label">Due now</span>
              </div>
              <div className="deck-stat">
                <span className="deck-stat-num">{newCount}</span>
                <span className="deck-stat-label">New</span>
              </div>
              <div className="deck-stat">
                <span className="deck-stat-num">{totalCount}</span>
                <span className="deck-stat-label">Total</span>
              </div>
            </div>
            <div className="deck-summary-progress" title={`${studiedCount} of ${totalCount} cards studied`}>
              <span>{studiedCount} / {totalCount} studied</span>
              <span className="deck-summary-track" aria-hidden="true">
                <span style={{ width: `${studiedPct}%` }} />
              </span>
              {directSubdecks.length === 0 && !addingSubdeck && (
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => setAddingSubdeck(true)}
                >
                  + Subdeck
                </button>
              )}
            </div>
          </section>
        )}
        {(directSubdecks.length > 0 || addingSubdeck) && (
          <section className="subdecks-block" aria-label="Subdecks">
            <div className="subdecks-block-head">
              <div className="subdecks-block-title">
                <h2>Subdecks</h2>
                {directSubdecks.length > 0 && (
                  <span className="today-decks-count">{directSubdecks.length}</span>
                )}
              </div>
              {!addingSubdeck && (
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => setAddingSubdeck(true)}
                >
                  + Subdeck
                </button>
              )}
            </div>
            {addingSubdeck && (
              <div className="subdeck-add-wrap">
                <form className="subdeck-add-form" onSubmit={(e) => void createSubdeck(e)}>
                  <input
                    className="text-input"
                    placeholder={`Subdeck inside ${leafLabel}…`}
                    value={subdeckName}
                    onChange={(e) => {
                      setSubdeckName(e.target.value)
                      setSubdeckError(null)
                    }}
                    autoFocus
                  />
                  <button type="submit" className="btn btn-primary btn-sm" disabled={!subdeckName.trim()}>
                    Create
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    onClick={() => {
                      setAddingSubdeck(false)
                      setSubdeckName('')
                      setSubdeckError(null)
                    }}
                  >
                    Cancel
                  </button>
                </form>
                {subdeckError && <p className="ds-row-sub ds-error" role="alert">{subdeckError}</p>}
              </div>
            )}
            {directSubdecks.length > 0 && (
              <nav className="deck-list subdecks-rows" aria-label="Subdecks list">
                {directSubdecks.map(({ node: child, deck: childDeck, displayLabel }) => {
                  const childSummary = summarizeDeckSubtree(child)
                  const hasNested = child.children.length > 0
                  const pct = child.total > 0 ? Math.min(1, child.studied / child.total) : 0
                  const ringCircumference = 2 * Math.PI * 7
                  const ringOffset = ringCircumference * (1 - pct)
                  const complete = child.total > 0 && child.studied >= child.total

                  const childMeta = (
                    <span className="deck-list-meta">
                      {child.due > 0 && <span className="deck-list-due has-due">{child.due} due</span>}
                      {child.total > 0 && child.studied > 0 ? (
                        <span
                          className={`deck-progress-ring${complete ? ' is-complete' : ''}`}
                          title={`${child.studied} of ${child.total} studied`}
                        >
                          <svg viewBox="0 0 18 18" aria-hidden="true">
                            <circle className="deck-progress-track" cx="9" cy="9" r="7" />
                            <circle
                              className="deck-progress-fill"
                              cx="9"
                              cy="9"
                              r="7"
                              strokeDasharray={ringCircumference}
                              strokeDashoffset={ringOffset}
                            />
                          </svg>
                          <span>
                            {child.studied} / {child.total} studied
                          </span>
                        </span>
                      ) : (
                        <span>
                          {child.total} {child.total === 1 ? 'card' : 'cards'}
                        </span>
                      )}
                    </span>
                  )

                  return (
                    <div key={childDeck.id} className="deck-list-row subdeck-list-row">
                      <Link to={`/decks/${childDeck.id}`} className="deck-list-link">
                        {hasNested ? (
                          <svg className="deck-icon deck-list-icon deck-folder-icon" viewBox="0 0 16 16" aria-hidden="true">
                            <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.6a1.5 1.5 0 0 1 1.15.54l.65.78a1 1 0 0 0 .77.36h3.83A1.5 1.5 0 0 1 14 6.18v5.32A1.5 1.5 0 0 1 12.5 13h-9A1.5 1.5 0 0 1 2 11.5v-7Z" />
                          </svg>
                        ) : (
                          <svg className="deck-icon deck-list-icon" viewBox="0 0 16 16" aria-hidden="true">
                            <rect x="2.5" y="3.5" width="10" height="8" rx="1.5" />
                            <path d="M4.5 1.5h7a2 2 0 0 1 2 2v6" />
                          </svg>
                        )}
                        <span className="subdeck-row-name-wrap">
                          <span className="deck-list-name" title={childDeck.name}>{displayLabel}</span>
                          {childSummary.subdeckCount > 0 && (
                            <span className="subdeck-row-count">
                              {childSummary.subdeckCount} {childSummary.subdeckCount === 1 ? 'subdeck' : 'subdecks'}
                            </span>
                          )}
                        </span>
                        {childMeta}
                      </Link>
                      <div className="deck-list-actions">
                        {child.due > 0 ? (
                          <Link
                            to={`/decks/${childDeck.id}/study`}
                            className="btn btn-ghost btn-sm deck-list-study"
                            aria-label={`Study ${childDeck.name}`}
                          >
                            Study
                          </Link>
                        ) : (
                          <Link
                            to={`/decks/${childDeck.id}`}
                            className="deck-list-arrow"
                            aria-label={`Open ${childDeck.name}`}
                          >
                            ›
                          </Link>
                        )}
                      </div>
                    </div>
                  )
                })}
              </nav>
            )}
          </section>
        )}
        {cards.length === 0 ? (
          directSubdecks.length > 0 ? (
            <div className="subdecks-direct-footer">
              <span>
                {totalCount > 0
                  ? `All ${totalCount} ${totalCount === 1 ? 'card lives' : 'cards live'} in the subdecks above.`
                  : 'No cards in this deck or its subdecks yet.'}
              </span>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={() => navigate(`/decks/${deckId}/cards/new`)}
              >
                + Add direct card
              </button>
            </div>
          ) : (
            <div className="empty-state">
              <EmptyIcon kind="cards" />
              <h3>No cards yet</h3>
              <p>Add your first card — front, back, done.</p>
              <div className="empty-state-actions">
                <button className="btn btn-primary cta" onClick={() => navigate(`/decks/${deckId}/cards/new`)}>
                  + Add your first card
                </button>
                {!addingSubdeck && (
                  <button className="btn btn-ghost cta" onClick={() => setAddingSubdeck(true)}>
                    + Add subdeck
                  </button>
                )}
              </div>
            </div>
          )
        ) : (
          <>
            {directSubdecks.length > 0 && (
              <div className="direct-cards-head">
                <h2>Cards in this deck</h2>
                <span className="today-decks-count">
                  {cards.length} {cards.length === 1 ? 'card' : 'cards'}
                </span>
              </div>
            )}

            <div className="card-list-tools">
              <div className="card-search-wrap">
                <svg className="card-search-icon" viewBox="0 0 16 16" aria-hidden="true">
                  <circle cx="7.2" cy="7.2" r="4.3" />
                  <path d="m10.5 10.5 3 3" />
                </svg>
                <input
                  ref={searchRef}
                  type="search"
                  className="card-search-input"
                  placeholder="Search cards…"
                  aria-label="Search cards"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                />
              </div>
              <div className="card-status-pills" role="group" aria-label="Filter by status">
                {statusTabs.map((tab) => (
                  <button
                    key={tab.value}
                    type="button"
                    className={`filter-pill${statusFilter === tab.value ? ' is-active' : ''}`}
                    aria-pressed={statusFilter === tab.value}
                    onClick={() => setStatusFilter(tab.value)}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>
              {tagOptions.length > 0 && (
                <select
                  id="tag-filter"
                  className="card-tag-filter"
                  aria-label="Filter by tag"
                  value={tagFilter}
                  onChange={(event) => setTagFilter(event.target.value)}
                >
                  <option value="">All tags</option>
                  {tagOptions.map((tag) => <option key={tag} value={tag}>{tag}</option>)}
                </select>
              )}
              <span className="card-filter-count">
                {visibleCards.length} of {cards.length} cards
              </span>
            </div>

            {visibleCards.length === 0 && hasFilters ? (
              <div className="empty-state card-list-empty">
                <EmptyIcon kind="search" />
                <h3>No matching cards</h3>
                <p>Try a different search term or clear your active filters.</p>
                <button
                  type="button"
                  className="btn btn-ghost cta"
                  onClick={() => {
                    setQuery('')
                    setStatusFilter('all')
                    setTagFilter('')
                  }}
                >
                  Clear filters
                </button>
              </div>
            ) : (
              <section className="card-list" aria-label="Cards">
                {visibleCards.map((card) => {
                  const status = cardStatus(card, now)
                  const tags = userTags(card.tags)
                  const backSnippet = cardSnippet(card.back)
                  const frontText = cardPreview(card.front)
                  return (
                    <div key={card.id} className="card-row">
                      <button
                        type="button"
                        className="card-row-main"
                        onClick={() => navigate(`/decks/${deckId}/cards/${card.id}/edit`)}
                      >
                        <span className="card-front">
                          {card.clozeIndex && (
                            <span className="card-cloze-badge">
                              {hasImageOcclusion(card.front) ? `IO ${card.clozeIndex}` : `c${card.clozeIndex}`}
                            </span>
                          )}
                          {frontText || <span className="muted">Untitled card</span>}
                        </span>
                        {(backSnippet.text || backSnippet.lang) && (
                          <span className="card-back">
                            {backSnippet.lang && (
                              <span className="card-lang-pill">{backSnippet.lang}</span>
                            )}
                            <span className={backSnippet.lang ? 'card-back-code' : 'card-back-text'}>
                              {backSnippet.text}
                            </span>
                          </span>
                        )}
                      </button>

                      <div className="card-row-actions">
                        <button
                          type="button"
                          className="card-row-action"
                          onClick={() => navigate(`/decks/${deckId}/cards/${card.id}/edit`)}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          className="card-row-action"
                          onClick={() => void toggleSuspend(card)}
                        >
                          {card.suspended ? 'Unsuspend' : 'Suspend'}
                        </button>
                      </div>

                      <div className="card-row-meta">
                        {tags.length > 0 && (
                          <span className="card-user-tags" aria-label={`Tags: ${tags.join(', ')}`}>
                            {tags.map((tag) => <span key={tag} className="card-user-tag">{tag}</span>)}
                          </span>
                        )}
                        <span className={`status-tag status-${status.kind}`}>{status.label}</span>
                      </div>
                    </div>
                  )
                })}
              </section>
            )}
          </>
        )}
      </div>
    </>
  )
}
