import { useEffect, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import type { Card } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
import { EmptyIcon } from '../../ui/EmptyIcon'
import { PageHeader } from '../../ui/PageHeader'
import { isNew } from '../review/dueOverview'
import { userTags } from './cardTags'

/** First non-empty line of markdown, reduced to plain text for a one-line card preview. */
export function cardPreview(md: string): string {
  const line = md.split('\n').find((l) => l.trim())?.trim() ?? ''
  return line
    .replace(/^#{1,6}\s+/, '') // heading
    .replace(/^>\s?/, '') // blockquote
    .replace(/^[-*+]\s+/, '') // bullet list
    .replace(/^\d+\.\s+/, '') // ordered list
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1') // link -> text
    .replace(/(\*\*|__|~~|[*_`])/g, '') // emphasis / inline-code markers
    .trim()
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

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const cards = useStorageQuery(() => (deckId ? storage.listCards(deckId) : []), [deckId])
  const due = useStorageQuery(() => (deckId ? storage.countDue(deckId, Date.now()) : 0), [deckId])

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
  const newCount = cards.filter((c) => isNew(c.scheduling)).length
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

  const title = <span className="header-title-text">{deck.name}</span>

  const actions = (
    <>
      <button className="btn btn-ghost" onClick={() => navigate(`/decks/${deckId}/options`)}>
        Options
      </button>
      {cards.length > 0 && (
        <>
          <button className="btn btn-ghost" onClick={() => navigate(`/decks/${deckId}/cards/new`)}>
            + Add card
          </button>
          {due && due > 0 ? (
            <Link to={`/decks/${deckId}/study`} className="btn btn-primary">
              Study {due}
            </Link>
          ) : (
            <>
              <span className="header-caught-up muted">All caught up today</span>
              <button
                className="btn btn-ghost"
                onClick={() => navigate(`/decks/${deckId}/options#custom-study`)}
              >
                Custom study
              </button>
            </>
          )}
        </>
      )}
    </>
  )

  return (
    <>
      <PageHeader title={title} actions={actions} />
      <div className="page-body">
        {cards.length === 0 ? (
          <div className="empty-state">
            <EmptyIcon kind="cards" />
            <h3>No cards yet</h3>
            <p>Add your first card — front, back, done.</p>
            <button className="btn btn-primary cta" onClick={() => navigate(`/decks/${deckId}/cards/new`)}>
              + Add your first card
            </button>
          </div>
        ) : (
          <>
            <section className="deck-stats" aria-label="Deck summary">
              <div className="deck-stat deck-stat-due">
                <div className="deck-stat-num">{due ?? 0}</div>
                <div className="deck-stat-label">Due now</div>
              </div>
              <div className="deck-stat">
                <div className="deck-stat-num">{newCount}</div>
                <div className="deck-stat-label">New</div>
              </div>
              <div className="deck-stat">
                <div className="deck-stat-num">{cards.length}</div>
                <div className="deck-stat-label">Total</div>
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
              <select
                className="card-tag-filter"
                aria-label="Filter by status"
                value={statusFilter}
                onChange={(event) => setStatusFilter(event.target.value as StatusFilter)}
              >
                <option value="all">All status</option>
                <option value="due">Due &amp; new</option>
                <option value="new">New only</option>
                <option value="suspended">Suspended</option>
              </select>
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
                  return (
                    <button
                      key={card.id}
                      className="card-row"
                      onClick={() => navigate(`/decks/${deckId}/cards/${card.id}/edit`)}
                    >
                      <span className="card-front">
                        {cardPreview(card.front) || <span className="muted">Untitled card</span>}
                      </span>
                      <span className="card-back">{cardPreview(card.back)}</span>
                      {tags.length > 0 && (
                        <span className="card-user-tags" aria-label={`Tags: ${tags.join(', ')}`}>
                          {tags.map((tag) => <span key={tag} className="card-user-tag">{tag}</span>)}
                        </span>
                      )}
                      <span className={`status-tag status-${status.kind}`}>{status.label}</span>
                    </button>
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
