import { useEffect, useRef, useState, type FormEvent } from 'react'
import { Link, useNavigate, useLocation } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { deckColor } from '../../ui/deckColor'
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

  const overview = useLiveQuery(() => loadDueOverview(storage, Date.now()), [])

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
      : `You have ${totalDue} card${totalDue === 1 ? '' : 's'} due across ${deckCount} deck${deckCount === 1 ? '' : 's'}.`

  return (
    <>
      <PageHeader title="Today" actions={<span className="header-date">{todayDate()}</span>} />
      <div className="today">
        <div className="today-inner">
          <h1 className="today-greet">{greeting(new Date().getHours())}</h1>
          <p className="today-sub">{sub}</p>

          {loaded &&
            (totalDue > 0 ? (
              <div className="review-band">
                <div className="review-band-left">
                  <div className="band-figure">
                    <span className="band-num">{totalDue}</span>
                    <span className="band-cap">
                      cards due
                      <br />
                      right now
                    </span>
                  </div>
                  <div className="band-chips">
                    <span className="chip chip-new">{overview?.totalNew ?? 0} NEW</span>
                    <span className="chip chip-review">{overview?.totalReview ?? 0} REVIEW</span>
                  </div>
                </div>
                <Link to="/study" className="band-start">
                  <span className="band-start-title">Start review</span>
                  <span className="band-start-hint">press ⏎</span>
                </Link>
              </div>
            ) : deckCount > 0 ? (
              <div className="caught-up">
                <span className="caught-up-title">All caught up.</span>
                <span className="caught-up-sub">Nothing due right now — add cards or come back later.</span>
              </div>
            ) : null)}

          {deckCount > 0 && (
            <>
              <div className="today-decks-head">
                <h2>Your decks</h2>
                <span className="today-decks-count">
                  {deckCount} deck{deckCount === 1 ? '' : 's'}
                </span>
              </div>
              <div className="deck-grid">
                {decks.map(({ deck, due, newCount, total }) => (
                  <Link key={deck.id} to={`/decks/${deck.id}`} className="deck-card">
                    <span className="deck-card-bar" style={{ background: deck.color ?? deckColor(deck.id) }} />
                    <span className="deck-card-body">
                      <span className="deck-card-meta">
                        <span className="algo-chip">FSRS</span>
                        <span className="deck-card-total">{total}</span>
                      </span>
                      <span className="deck-card-name">{deck.name}</span>
                      <span className="deck-card-foot">
                        <span className="deck-card-due">{due} due</span>
                        <span className="deck-card-new">{newCount} new</span>
                      </span>
                    </span>
                  </Link>
                ))}
              </div>
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
              <div className="ico">🗂️</div>
              <h3>No decks yet</h3>
              <p>Name a deck above to start building your memory.</p>
            </div>
          )}
        </div>
      </div>
    </>
  )
}
