import { useState } from 'react'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { Grade } from '../../domain/models'
import { PageHeader } from '../../ui/PageHeader'
import { SegToggle } from '../../ui/SegToggle'
import { buildStats, type StatsRange } from './stats'

const GRADE_META: Array<{ grade: Grade; label: string; className: string }> = [
  { grade: 'again', label: 'Again', className: 'is-again' },
  { grade: 'hard', label: 'Hard', className: 'is-hard' },
  { grade: 'good', label: 'Good', className: 'is-good' },
  { grade: 'easy', label: 'Easy', className: 'is-easy' },
]

type RangeKey = '7' | '30' | '90'

export function StatsPage() {
  const storage = useStorage()
  const [deckId, setDeckId] = useState('all')
  const [rangeKey, setRangeKey] = useState<RangeKey>('30')
  const loaded = useLiveQuery(async () => {
    const decks = await storage.listDecks()
    const logs = (await Promise.all(decks.map((deck) => storage.listReviewLogs(deck.id)))).flat()
    return { decks, logs }
  }, [storage])

  const controls = (
    <div className="stats-controls">
      <select
        className="stats-deck-filter"
        aria-label="Deck filter"
        value={deckId}
        onChange={(event) => setDeckId(event.target.value)}
      >
        <option value="all">All decks</option>
        {(loaded?.decks ?? []).map((deck) => (
          <option key={deck.id} value={deck.id}>{deck.name}</option>
        ))}
      </select>
      <SegToggle
        value={rangeKey}
        onChange={setRangeKey}
        options={[
          { value: '7', label: '7D' },
          { value: '30', label: '30D' },
          { value: '90', label: '90D' },
        ]}
      />
    </div>
  )

  if (!loaded) return <PageHeader title="Stats" actions={controls} />

  const selectedDeck = loaded.decks.some((deck) => deck.id === deckId) ? deckId : 'all'
  const range = Number(rangeKey) as StatsRange
  const stats = buildStats(loaded.logs, loaded.decks, Date.now(), range, selectedDeck === 'all' ? null : selectedDeck)
  const maxDaily = Math.max(1, ...stats.daily.map((day) => day.count))
  const chartLabel = `Daily review activity: ${stats.totalReviews} reviews over ${range} days`

  return (
    <>
      <PageHeader title="Stats" actions={controls} />
      <div className="stats-page">
        {stats.historyCount === 0 ? (
          <div className="empty-state stats-empty">
            <div className="ico">▥</div>
            <h3>No review history yet</h3>
            <p>Complete FSRS reviews to start building your activity history.</p>
          </div>
        ) : stats.totalReviews === 0 ? (
          <div className="empty-state stats-empty">
            <div className="ico">○</div>
            <h3>No activity in this range</h3>
            <p>Try a longer range or another deck.</p>
          </div>
        ) : (
          <>
            <div className="stats-kpis">
              <Kpi label="FSRS reviews" value={String(stats.totalReviews)} />
              <Kpi label="Recall rate" value={percent(stats.recallRate ?? 0)} />
              <Kpi
                label="Current streak"
                value={`${stats.currentStreak} day${stats.currentStreak === 1 ? '' : 's'}`}
              />
              <Kpi label="Active days" value={String(stats.activeDays)} />
            </div>

            <section className="stats-panel stats-activity">
              <div className="stats-panel-head">
                <div>
                  <h2>Review activity</h2>
                  <p>FSRS reviews completed per local calendar day.</p>
                </div>
                <strong>{stats.totalReviews}</strong>
              </div>
              <div className="stats-chart" role="img" aria-label={chartLabel}>
                {stats.daily.map((day) => (
                  <span
                    key={day.day}
                    className={day.count > 0 ? 'stats-bar has-activity' : 'stats-bar'}
                    style={{ height: `${day.count === 0 ? 2 : Math.max(8, (day.count / maxDaily) * 100)}%` }}
                    title={`${formatDay(day.timestamp)}: ${day.count} review${day.count === 1 ? '' : 's'}`}
                  />
                ))}
              </div>
              <div className="stats-axis" aria-hidden="true">
                <span>{formatDay(stats.daily[0].timestamp)}</span>
                <span>{formatDay(stats.daily[Math.floor(stats.daily.length / 2)].timestamp)}</span>
                <span>{formatDay(stats.daily.at(-1)!.timestamp)}</span>
              </div>
            </section>

            <div className="stats-detail-grid">
              <section className="stats-panel">
                <div className="stats-panel-head">
                  <div>
                    <h2>Grade distribution</h2>
                    <p>Again counts as a miss for recall rate.</p>
                  </div>
                </div>
                <div className="stats-grades">
                  {GRADE_META.map(({ grade, label, className }) => {
                    const count = stats.grades[grade]
                    const share = count / stats.totalReviews
                    return (
                      <div key={grade} className="stats-grade" aria-label={`${label} grade`}>
                        <span className="stats-grade-label">{label}</span>
                        <span className="stats-grade-track">
                          <span className={`stats-grade-fill ${className}`} style={{ width: `${share * 100}%` }} />
                        </span>
                        <strong>{count}</strong>
                        <span>{percent(share)}</span>
                      </div>
                    )
                  })}
                </div>
              </section>

              <section className="stats-panel">
                <div className="stats-panel-head">
                  <div>
                    <h2>By deck</h2>
                    <p>Activity and recall in this range.</p>
                  </div>
                </div>
                <div className="stats-decks">
                  {stats.byDeck.map((item) => (
                    <div key={item.deck.id} className="stats-deck-row" aria-label={`${item.deck.name} deck stats`}>
                      <span className="stats-deck-dot" style={{ background: item.deck.color }} />
                      <span className="stats-deck-name">{item.deck.name}</span>
                      <strong>{item.reviews}</strong>
                      <span>{percent(item.recallRate)}</span>
                    </div>
                  ))}
                </div>
              </section>
            </div>
          </>
        )}

        <p className="stats-scope-note">
          Stats begin when review history was enabled. They include FSRS-effective grades, not fixed learning-step clicks.
        </p>
      </div>
    </>
  )
}

function Kpi({ label, value }: { label: string; value: string }) {
  return (
    <div className="stats-kpi" aria-label={label}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

function percent(value: number): string {
  return `${Math.round(value * 100)}%`
}

function formatDay(timestamp: number): string {
  return new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}
