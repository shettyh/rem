import { useEffect, useRef, useState, type FormEvent } from 'react'
import { Link, useNavigate, useLocation } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import { PageHeader } from '../../ui/PageHeader'
import { loadDueOverview } from '../review/dueOverview'

/** Time-of-day greeting for the Today header. */
function greeting(hour: number): string {
  if (hour < 5) return 'Still up.'
  if (hour < 12) return 'Good morning.'
  if (hour < 18) return 'Good afternoon.'
  return 'Good evening.'
}

function todayDate(): string {
  return new Date().toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' })
}

export function DeckListPage() {
  const storage = useStorage()
  const navigate = useNavigate()
  const location = useLocation()
  const newDeckRef = useRef<HTMLInputElement>(null)
  const [name, setName] = useState('')

  const overview = useStorageQuery(() => loadDueOverview(storage, Date.now()), [])

  const totalDue = overview?.totalDue ?? 0

  // Focus the new-deck input when arriving via the sidebar "+".
  useEffect(() => {
    if ((location.state as { focusNewDeck?: number } | null)?.focusNewDeck) {
      newDeckRef.current?.scrollIntoView({ block: 'center' })
      newDeckRef.current?.focus()
    }
  }, [location.state])

  // Enter starts the cross-deck review session when something is due.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement | null)?.tagName ?? ''
      if (tag === 'INPUT' || tag === 'TEXTAREA') return
      if (e.key === 'Enter' && totalDue > 0) {
        e.preventDefault()
        navigate('/study')
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [totalDue, navigate])

  async function addDeck(e: FormEvent) {
    e.preventDefault()
    const trimmed = name.trim()
    if (!trimmed) return
    await storage.createDeck(trimmed)
    setName('')
  }

  const loaded = overview !== undefined
  const decks = overview?.decks ?? []
  const deckCount = decks.length
  const sub = !loaded
    ? ''
    : deckCount === 0
      ? 'Create your first deck to start remembering.'
      : totalDue > 0
        ? `${totalDue} card${totalDue === 1 ? '' : 's'} due across ${deckCount} deck${deckCount === 1 ? '' : 's'}.`
        : `All caught up across ${deckCount} deck${deckCount === 1 ? '' : 's'}.`

  return (
    <>
      <PageHeader title="Today" actions={<span className="header-date">{todayDate()}</span>} />
      <div className="today">
        <div className="today-inner">
          <h1 className="today-greet">{greeting(new Date().getHours())}</h1>
          <p className="today-sub">{sub}</p>

          {loaded && totalDue > 0 && (
            <section className="review-summary" aria-label="Review summary">
              <div className="review-summary-copy">
                <span className="review-summary-number">{totalDue}</span>
                <span className="review-summary-label">
                  <strong>{totalDue === 1 ? 'card due' : 'cards due'}</strong>
                  <span>
                    {overview?.totalNew ?? 0} new · {overview?.totalReview ?? 0} review
                  </span>
                </span>
              </div>
              <Link to="/study" className="btn btn-primary review-summary-action">
                Start review <span className="kbd">↵</span>
              </Link>
            </section>
          )}

          {deckCount > 0 && (
            <>
              <div className="today-decks-head">
                <h2>Your decks</h2>
                <span className="today-decks-count">
                  {deckCount} deck{deckCount === 1 ? '' : 's'}
                </span>
              </div>
              <nav className="deck-list" aria-label="Decks">
                {decks.map(({ deck, due, newCount, total }) => (
                  <Link key={deck.id} to={`/decks/${deck.id}`} className="deck-list-row">
                    <span className="deck-list-name">{deck.name}</span>
                    <span className="deck-list-meta">
                      {due > 0 && <span className="deck-list-due has-due">{due} due</span>}
                      {newCount > 0 && <span>{newCount} new</span>}
                      <span>{total} {total === 1 ? 'card' : 'cards'}</span>
                      <span className="deck-list-arrow" aria-hidden="true">›</span>
                    </span>
                  </Link>
                ))}
              </nav>
            </>
          )}

          <form className="add-row" onSubmit={addDeck}>
            <input
              ref={newDeckRef}
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

          {loaded && deckCount === 0 && (
            <div className="empty-state">
              <div className="ico" aria-hidden="true">DECKS</div>
              <h3>No decks yet</h3>
              <p>Name a deck above to start building your memory.</p>
            </div>
          )}
        </div>
      </div>
    </>
  )
}
