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
      all.map(async (deck) => ({ deck, due: await storage.countDue(deck.id, now) })),
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
      <div className="row between">
        <h1 className="page-title">Decks</h1>
      </div>

      <form className="row" onSubmit={addDeck}>
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
        <p className="empty">No decks yet. Create one above to start.</p>
      ) : (
        <div className="stack">
          {decks.map(({ deck, due }) => (
            <Link key={deck.id} to={`/decks/${deck.id}`} className="list-row">
              <span>{deck.name}</span>
              <span className={due > 0 ? 'badge' : 'badge badge-zero'}>{due} due</span>
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
