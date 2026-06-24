import { Link, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { SchedulingState } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
import { PageHeader } from '../../ui/PageHeader'

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
  const isNew = s.kind === 'sm2' ? s.repetitions === 0 : s.reps === 0
  if (isNew) return { kind: 'new', label: 'new' }
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

  const title = (
    <>
      <span className="header-title-text">{deck.name}</span>
      <span className="sched-badge">{deck.schedulerKind === 'fsrs' ? 'FSRS' : 'SM-2'}</span>
    </>
  )

  const actions =
    cards.length === 0 ? undefined : (
      <>
        {due && due > 0 ? (
          <Link to={`/decks/${deckId}/study`} className="btn btn-primary">
            Study {due}
          </Link>
        ) : (
          <span className="muted">All caught up today</span>
        )}
        <Link to={`/decks/${deckId}/cards/new`} className="btn btn-ghost">
          + Add card
        </Link>
      </>
    )

  return (
    <>
      <PageHeader title={title} actions={actions} />
      <div className="page-body stack">
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
                    {cardPreview(card.front) || <span className="muted">Untitled card</span>}
                  </span>
                  <span className={`status-tag status-${status.kind}`}>{status.label}</span>
                  <span className="card-edit">edit</span>
                </Link>
              )
            })}
          </div>
        )}
      </div>
    </>
  )
}
