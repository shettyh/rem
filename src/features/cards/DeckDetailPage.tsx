import { Link, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { SchedulingState } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'

/** First non-empty line of markdown, used as a one-line card preview. */
function firstLine(md: string): string {
  return md.split('\n').find((l) => l.trim())?.trim() ?? ''
}

/** A card's review status, derived from existing scheduling state. */
export function cardStatus(
  s: SchedulingState,
  now: number,
): { kind: 'new' | 'due' | 'scheduled'; label: string } {
  if (s.repetitions === 0) return { kind: 'new', label: 'new' }
  if (s.due <= now) return { kind: 'due', label: 'due' }
  const days = Math.max(1, Math.round((s.due - now) / MS_PER_DAY))
  const label = days >= 30 ? `${Math.round(days / 30)}mo` : `${days}d`
  return { kind: 'scheduled', label }
}

export function DeckDetailPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const cards = useLiveQuery(() => (deckId ? storage.listCards(deckId) : []), [deckId])
  const due = useLiveQuery(() => (deckId ? storage.countDue(deckId, Date.now()) : 0), [deckId])

  if (!deckId || deck === undefined || cards === undefined) return null

  const now = Date.now()

  return (
    <div className="stack">
      <div className="row between">
        <h1 className="page-title">{deck.name}</h1>
        {cards.length === 0 ? null : due && due > 0 ? (
          <Link to={`/decks/${deckId}/study`} className="btn btn-primary">
            Study {due}
          </Link>
        ) : (
          <span className="caught-up">All caught up today</span>
        )}
      </div>

      {cards.length === 0 ? (
        <div className="empty-state">
          <div className="ico">✏️</div>
          <h3>No cards yet</h3>
          <p>Add your first card — front, back, done.</p>
          <Link to={`/decks/${deckId}/cards/new`} className="btn btn-primary cta">
            + Add your first card
          </Link>
        </div>
      ) : (
        <>
          <div className="add-row">
            <span className="muted">
              {cards.length} card{cards.length === 1 ? '' : 's'}
            </span>
            <Link to={`/decks/${deckId}/cards/new`} className="btn btn-ghost">
              + Add card
            </Link>
          </div>
          <div className="stack">
            {cards.map((card) => {
              const status = cardStatus(card.scheduling, now)
              return (
                <Link
                  to={`/decks/${deckId}/cards/${card.id}`}
                  className="card-row"
                  key={card.id}
                >
                  <span className="card-front">
                    {firstLine(card.front) || <span className="muted">Untitled card</span>}
                  </span>
                  <span className={`status-tag status-${status.kind}`}>{status.label}</span>
                  <span className="card-edit">edit</span>
                </Link>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}
