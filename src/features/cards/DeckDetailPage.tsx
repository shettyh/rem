import { useEffect, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import type { Card } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
import { EmptyIcon } from '../../ui/EmptyIcon'
import { PageHeader } from '../../ui/PageHeader'
import { isNew, loadDueOverview } from '../review/dueOverview'
import { deckIsDescendantOrSelf } from '../decks/deckTree'
import { userTags } from './cardTags'
import { stripClozeMarkup } from './cloze'
import { hasImageOcclusion, stripOcclusionMarkup } from './occlusion'

/** Clean a single line of markdown into plain preview text. */
function cleanMarkdownLine(line: string): string {
  return stripClozeMarkup(line)
    .replace(/^#{1,6}\s+/, '') // heading
    .replace(/^>\s?/, '') // blockquote
    .replace(/^[-*+]\s+/, '') // bullet list
    .replace(/^\d+\.\s+/, '') // ordered list
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1') // link -> text
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

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const cards = useStorageQuery(() => (deckId ? storage.listCards(deckId) : []), [deckId])
  const subdecksInfo = useStorageQuery(async () => {
    if (!deckId) {
      return {
        totalDue: 0,
        rolledNew: 0,
        rolledTotal: 0,
        rolledStudied: 0,
        subdecks: [] as { deck: Awaited<ReturnType<typeof storage.listDecks>>[number]; due: number }[],
      }
    }
    const overview = await loadDueOverview(storage, Date.now())
    const current = overview.decks.find((entry) => entry.deck.id === deckId)
    if (!current) {
      return { totalDue: 0, rolledNew: 0, rolledTotal: 0, rolledStudied: 0, subdecks: [] }
    }
    const descendants = overview.decks.filter(
      (entry) =>
        entry.deck.id !== current.deck.id
        && deckIsDescendantOrSelf(entry.deck.name, current.deck.name),
    )
    const subdecks = descendants.map((entry) => ({ deck: entry.deck, due: entry.due }))
    const totalDue = current.due + descendants.reduce((acc, s) => acc + s.due, 0)
    const rolledTotal = current.total + descendants.reduce((acc, s) => acc + s.total, 0)
    const rolledStudied = current.studied + descendants.reduce((acc, s) => acc + s.studied, 0)
    const rolledNew = Math.max(0, rolledTotal - rolledStudied)
    return { totalDue, rolledNew, rolledTotal, rolledStudied, subdecks }
  }, [deckId])
  const due = subdecksInfo?.totalDue ?? 0
  const subdecks = subdecksInfo?.subdecks ?? []

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

  if (!deckId || deck === undefined || cards === undefined) return null

  const now = Date.now()
  const directNewCount = cards.filter((c) => isNew(c.scheduling)).length
  const totalCount = subdecks.length > 0 ? (subdecksInfo?.rolledTotal ?? cards.length) : cards.length
  const newCount = subdecks.length > 0 ? (subdecksInfo?.rolledNew ?? directNewCount) : directNewCount
  const studiedCount =
    subdecks.length > 0 ? (subdecksInfo?.rolledStudied ?? (cards.length - directNewCount)) : cards.length - directNewCount
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

  const title = (
    <>
      <span className="header-title-text">{deck.name}</span>
      {(cards.length > 0 || subdecks.length > 0) && (
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
      {(cards.length > 0 || subdecks.length > 0) && (
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
      )}
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
        {(subdecks.length > 0 || addingSubdeck) && (
          <section className="subdecks-section" aria-label="Subdecks">
            <div className="subdecks-head">
              <span className="subdecks-title">Subdecks</span>
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
              <>
                <form className="subdeck-add-form" onSubmit={(e) => void createSubdeck(e)}>
                  <input
                    className="text-input"
                    placeholder={`Subdeck inside ${deck.name}…`}
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
              </>
            )}
            {subdecks.length > 0 && (
              <div className="subdecks-list">
                {subdecks.map(({ deck: child, due: childDue }) => {
                  const prefix = `${deck.name}::`
                  const relLabel = child.name.startsWith(prefix)
                    ? child.name.slice(prefix.length)
                    : child.name
                  return (
                    <Link key={child.id} to={`/decks/${child.id}`} className="subdeck-chip">
                      <span>{relLabel}</span>
                      {childDue > 0 && <span className="subdeck-chip-due">{childDue} due</span>}
                    </Link>
                  )
                })}
              </div>
            )}
          </section>
        )}
        {cards.length === 0 ? (
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
        ) : (
          <>
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
                {subdecks.length === 0 && !addingSubdeck && (
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
