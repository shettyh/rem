import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'

export function DeckListPage() {
  const storage = useStorage()
  const [name, setName] = useState('')

  const decks = useLiveQuery(async () => {
    const all = await storage.listDecks()
    const now = Date.now()
    return Promise.all(
      all.map(async (deck) => ({
        deck,
        due: await storage.countDue(deck.id, now),
        count: (await storage.listCards(deck.id)).length,
      })),
    )
  }, [])

  async function addDeck(e: FormEvent) {
    e.preventDefault()
    const trimmed = name.trim()
    if (!trimmed) return
    await storage.createDeck(trimmed)
    setName('')
  }

  return (
    <div className="stack">
      <h1 className="page-title">Decks</h1>

      <form className="add-row" onSubmit={addDeck}>
        <input
          className="text-input"
          placeholder="New deck name…"
          value={name}
          onChange={(e) => setName(e.target.value)}
          aria-label="New deck name"
        />
        <button className="btn btn-primary" type="submit" disabled={!name.trim()}>
          Add deck
        </button>
      </form>

      {decks === undefined ? null : decks.length === 0 ? (
        <div className="empty-state">
          <div className="ico">🗂️</div>
          <h3>No decks yet</h3>
          <p>Name a deck above to start building your memory.</p>
        </div>
      ) : (
        <div className="stack">
          {decks.map(({ deck, due, count }) => (
            <Link key={deck.id} to={`/decks/${deck.id}`} className="deck-row">
              <div className="deck-text">
                <span className="deck-name">{deck.name}</span>
                <span className="deck-meta">
                  {count} card{count === 1 ? '' : 's'}
                </span>
              </div>
              {due > 0 ? (
                <span className="due-chip">{due} due</span>
              ) : (
                <span className="due-none">All caught up</span>
              )}
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
