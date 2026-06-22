import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { SchedulerKind } from '../../domain/models'

/** Time-of-day greeting for the home header. */
function greeting(hour: number): string {
  if (hour < 12) return 'Good morning.'
  if (hour < 18) return 'Good afternoon.'
  return 'Good evening.'
}

export function DeckListPage() {
  const storage = useStorage()
  const [name, setName] = useState('')
  const [kind, setKind] = useState<SchedulerKind>('fsrs')

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
    await storage.createDeck(trimmed, kind)
    setName('')
  }

  const totalDue = decks?.reduce((n, d) => n + d.due, 0) ?? 0
  const deckCount = decks?.length ?? 0
  const dueLine =
    totalDue > 0
      ? `You have ${totalDue} card${totalDue === 1 ? '' : 's'} due today.`
      : "You're all caught up — nothing due today."

  return (
    <div className="stack">
      {decks && decks.length > 0 && (
        <header className="home-hero">
          <div>
            <p className="hero-greet">{greeting(new Date().getHours())}</p>
            <p className="hero-sub">{dueLine}</p>
          </div>
          <div className="hero-stats">
            <div className="stat stat-due">
              <span className="stat-num">{totalDue}</span>
              <span className="stat-label">due today</span>
            </div>
            <div className="stat">
              <span className="stat-num">{deckCount}</span>
              <span className="stat-label">deck{deckCount === 1 ? '' : 's'}</span>
            </div>
          </div>
        </header>
      )}

      <h1 className="page-title">Decks</h1>

      <form className="add-row" onSubmit={addDeck}>
        <input
          className="text-input"
          placeholder="New deck name…"
          value={name}
          onChange={(e) => setName(e.target.value)}
          aria-label="New deck name"
        />
        <select
          className="text-input sched-picker"
          value={kind}
          onChange={(e) => setKind(e.target.value as SchedulerKind)}
          aria-label="Scheduler"
        >
          <option value="fsrs">FSRS (recommended)</option>
          <option value="sm2">SM-2</option>
        </select>
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
