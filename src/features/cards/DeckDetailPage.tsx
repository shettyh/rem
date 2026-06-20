import { Link, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'

/** First non-empty line of markdown, used as a one-line card preview. */
function firstLine(md: string): string {
  return md.split('\n').find((l) => l.trim())?.trim() ?? ''
}

export function DeckDetailPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const deck = useLiveQuery(
    () => (deckId ? storage.getDeck(deckId) : undefined),
    [deckId],
  )
  const cards = useLiveQuery(
    () => (deckId ? storage.listCards(deckId) : []),
    [deckId],
  )
  const due = useLiveQuery(
    () => (deckId ? storage.countDue(deckId, Date.now()) : 0),
    [deckId],
  )

  if (!deckId || deck === undefined || cards === undefined) return null

  return (
    <div className="stack">
      <div className="row between">
        <h1 className="page-title">{deck.name}</h1>
        {due && due > 0 ? (
          <Link to={`/decks/${deckId}/study`} className="btn btn-primary">
            Study ({due})
          </Link>
        ) : (
          <span className="btn" aria-disabled="true" style={{ opacity: 0.5 }}>
            Nothing due
          </span>
        )}
      </div>

      <div className="row">
        <Link to={`/decks/${deckId}/cards/new`} className="btn">
          + Add card
        </Link>
        <span className="muted">{cards.length} card{cards.length === 1 ? '' : 's'}</span>
      </div>

      {cards.length === 0 ? (
        <p className="empty">No cards yet. Add your first card above.</p>
      ) : (
        <div className="stack">
          {cards.map((card) => (
            <div className="list-row" key={card.id}>
              <Link to={`/decks/${deckId}/cards/${card.id}`} className="card-snippet">
                {firstLine(card.front) || <span className="muted">Untitled card</span>}
              </Link>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
