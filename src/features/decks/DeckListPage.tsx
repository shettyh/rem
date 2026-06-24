import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { SchedulerKind } from '../../domain/models'
import { PageHeader } from '../../ui/PageHeader'

/** Time-of-day greeting for the Today header. */
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
      all.map(async (deck) => ({ deck, due: await storage.countDue(deck.id, now) })),
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
  const topDeck = [...(decks ?? [])].filter((d) => d.due > 0).sort((a, b) => b.due - a.due)[0]?.deck

  return (
    <>
      <PageHeader title="Today" />
      <div className="page-body measure stack">
        <header className="home-hero">
          <div>
            <p className="hero-greet">{greeting(new Date().getHours())}</p>
            <p className="hero-sub">{dueLine}</p>
          </div>
          {deckCount > 0 && (
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
          )}
        </header>

        {totalDue > 0 && topDeck && (
          <div className="start-card">
            <div>
              <strong>Ready to review</strong>
              <div className="muted">
                {totalDue} card{totalDue === 1 ? '' : 's'} due across your decks.
              </div>
            </div>
            <Link to={`/decks/${topDeck.id}/study`} className="btn btn-primary">
              Start review
            </Link>
          </div>
        )}

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

        {decks !== undefined && decks.length === 0 && (
          <div className="empty-state">
            <div className="ico">🗂️</div>
            <h3>No decks yet</h3>
            <p>Name a deck above to start building your memory.</p>
          </div>
        )}
      </div>
    </>
  )
}
