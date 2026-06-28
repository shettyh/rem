import { Link, useNavigate, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { SchedulingState } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
import { PageHeader } from '../../ui/PageHeader'
import { deckColor } from '../../ui/deckColor'
import { isNew } from '../review/dueOverview'

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

/** A card's review status, derived from existing scheduling state. */
export function cardStatus(
  s: SchedulingState,
  now: number,
): { kind: 'new' | 'due' | 'scheduled'; label: string } {
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

  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const cards = useLiveQuery(() => (deckId ? storage.listCards(deckId) : []), [deckId])
  const due = useLiveQuery(() => (deckId ? storage.countDue(deckId, Date.now()) : 0), [deckId])

  if (!deckId || deck === undefined || cards === undefined) return null

  const now = Date.now()
  const newCount = cards.filter((c) => isNew(c.scheduling)).length

  const title = (
    <>
      <span className="header-dot" style={{ background: deck.color ?? deckColor(deck.id) }} />
      <span className="header-title-text">{deck.name}</span>
      <span className="algo-chip">FSRS</span>
    </>
  )

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
            <div className="ico">✏️</div>
            <h3>No cards yet</h3>
            <p>Add your first card — front, back, done.</p>
            <button className="btn btn-primary cta" onClick={() => navigate(`/decks/${deckId}/cards/new`)}>
              + Add your first card
            </button>
          </div>
        ) : (
          <>
            <div className="deck-stats">
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
            </div>

            <div className="card-list">
              {cards.map((card) => {
                const status = cardStatus(card.scheduling, now)
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
                    <span className={`status-tag status-${status.kind}`}>{status.label}</span>
                  </button>
                )
              })}
            </div>
          </>
        )}
      </div>

    </>
  )
}
