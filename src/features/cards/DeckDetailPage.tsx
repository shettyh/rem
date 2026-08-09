import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import type { Card } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
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

export function DeckDetailPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
  const [tagFilter, setTagFilter] = useState('')

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const cards = useStorageQuery(() => (deckId ? storage.listCards(deckId) : []), [deckId])
  const due = useStorageQuery(() => (deckId ? storage.countDue(deckId, Date.now()) : 0), [deckId])

  if (!deckId || deck === undefined || cards === undefined) return null

  const now = Date.now()
  const newCount = cards.filter((c) => isNew(c.scheduling)).length
  const tagOptions = [...new Set(cards.flatMap((card) => userTags(card.tags)))]
    .sort((a, b) => a.localeCompare(b))
  const visibleCards = tagFilter
    ? cards.filter((card) => card.tags.includes(tagFilter))
    : cards

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
            <span className="muted">All caught up today</span>
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
            <div className="ico" aria-hidden="true">CARDS</div>
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

            {tagOptions.length > 0 && (
              <div className="card-list-tools">
                <label className="field-label" htmlFor="tag-filter">Filter by tag</label>
                <select
                  id="tag-filter"
                  className="card-tag-filter"
                  value={tagFilter}
                  onChange={(event) => setTagFilter(event.target.value)}
                >
                  <option value="">All tags</option>
                  {tagOptions.map((tag) => <option key={tag} value={tag}>{tag}</option>)}
                </select>
                <span className="card-filter-count">
                  {visibleCards.length} of {cards.length} cards
                </span>
              </div>
            )}

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
          </>
        )}
      </div>

    </>
  )
}
