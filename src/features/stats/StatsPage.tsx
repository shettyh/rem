import { useState } from 'react'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import type { CollectionStatsDaily, Grade } from '../../domain/models'
import { EmptyIcon } from '../../ui/EmptyIcon'
import { PageHeader } from '../../ui/PageHeader'
import { SegToggle } from '../../ui/SegToggle'

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
  const [hoveredDay, setHoveredDay] = useState<CollectionStatsDaily | null>(null)
  const range = Number(rangeKey)

  const loaded = useStorageQuery(async () => {
    const decks = await storage.listDecks()
    const effectiveDeckId = decks.some((deck) => deck.id === deckId) ? deckId : 'all'
    const stats = await storage.getStats(effectiveDeckId === 'all' ? null : effectiveDeckId, range)
    return { decks, stats }
  }, [storage, deckId, range])

  const controls = (
    <div className="stats-controls">
      <select
        className="stats-deck-filter"
        aria-label="Deck filter"
        value={deckId}
        onChange={(event) => {
          setDeckId(event.target.value)
          setHoveredDay(null)
        }}
      >
        <option value="all">All decks</option>
        {(loaded?.decks ?? []).map((deck) => (
          <option key={deck.id} value={deck.id}>{deck.name}</option>
        ))}
      </select>
      <SegToggle
        value={rangeKey}
        onChange={(next) => {
          setRangeKey(next)
          setHoveredDay(null)
        }}
        options={[
          { value: '7', label: '7D' },
          { value: '30', label: '30D' },
          { value: '90', label: '90D' },
        ]}
      />
    </div>
  )

  if (!loaded) return <PageHeader title="Stats" actions={controls} />

  const { stats } = loaded
  const maxDaily = Math.max(1, ...stats.daily.map((day) => day.count))
  const chartLabel = `Daily review activity: ${stats.totalReviews} reviews over ${range} days`

  const forecast = stats.upcoming7d.map((day, index) => ({
    key: day.day,
    label: index === 0
      ? 'Today'
      : new Date(day.timestampMs).toLocaleDateString(undefined, { weekday: 'short' }),
    count: day.count,
  }))
  const upcomingTotal = forecast.reduce((sum, day) => sum + day.count, 0)
  const maxUpcoming = Math.max(1, ...forecast.map((day) => day.count))

  return (
    <>
      <PageHeader title="Stats" actions={controls} />
      <div className="stats-page">
        {stats.historyCount === 0 ? (
          <div className="empty-state stats-empty">
            <EmptyIcon kind="stats" />
            <h3>No review history yet</h3>
            <p>Complete reviews to start tracking your recall rate and daily momentum.</p>
          </div>
        ) : stats.totalReviews === 0 ? (
          <div className="empty-state stats-empty">
            <EmptyIcon kind="stats" />
            <h3>No activity in this range</h3>
            <p>Try a longer range or another deck.</p>
          </div>
        ) : (
          <>
            <div className="stats-kpis">
              <Kpi label="FSRS reviews" displayLabel="Reviews" value={String(stats.totalReviews)} />
              <Kpi label="Recall rate" value={percent(stats.recallRate ?? 0)} />
              <Kpi
                label="Current streak"
                value={`${stats.currentStreakDays} day${stats.currentStreakDays === 1 ? '' : 's'}`}
              />
              <Kpi label="Active days" value={String(stats.activeDays)} />
            </div>

            <section className="stats-panel stats-activity">
              <div className="stats-panel-head">
                <div>
                  <h2>Review activity</h2>
                  <p>Reviews completed per local calendar day.</p>
                </div>
                <div className="stats-head-readout">
                  {hoveredDay ? (
                    <>
                      <strong>{hoveredDay.count}</strong>
                      <span>{formatDay(hoveredDay.timestampMs)}</span>
                    </>
                  ) : (
                    <>
                      <strong>{stats.totalReviews}</strong>
                      <span>last {range} days</span>
                    </>
                  )}
                </div>
              </div>
              <div
                className="stats-chart"
                role="img"
                aria-label={chartLabel}
                onMouseLeave={() => setHoveredDay(null)}
              >
                {stats.daily.map((day) => {
                  const active = hoveredDay?.day === day.day
                  return (
                    <span
                      key={day.day}
                      className={`stats-bar${day.count > 0 ? ' has-activity' : ''}${active ? ' is-hovered' : ''}`}
                      style={{ height: `${day.count === 0 ? 2 : Math.max(8, (day.count / maxDaily) * 100)}%` }}
                      title={`${formatDay(day.timestampMs)}: ${day.count} review${day.count === 1 ? '' : 's'}`}
                      onMouseEnter={() => setHoveredDay(day)}
                    />
                  )
                })}
              </div>
              <div className="stats-axis" aria-hidden="true">
                <span>{formatDay(stats.daily[0].timestampMs)}</span>
                <span>{formatDay(stats.daily[Math.floor(stats.daily.length / 2)].timestampMs)}</span>
                <span>{formatDay(stats.daily.at(-1)!.timestampMs)}</span>
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
                    <div key={item.deckId} className="stats-deck-row" aria-label={`${item.deckName} deck stats`}>
                      <span className="stats-deck-name">{item.deckName}</span>
                      <strong>{item.reviews}</strong>
                      <span>{percent(item.recallRate)}</span>
                    </div>
                  ))}
                </div>
              </section>
            </div>

            {upcomingTotal > 0 && (
              <section className="stats-panel stats-forecast" aria-label="Upcoming reviews">
                <div className="stats-panel-head">
                  <div>
                    <h2>Upcoming workload</h2>
                    <p>Scheduled reviews due over the next 7 days.</p>
                  </div>
                  <div className="stats-head-readout">
                    <strong>{upcomingTotal}</strong>
                    <span>next 7 days</span>
                  </div>
                </div>
                <div className="stats-forecast-grid">
                  {forecast.map((day) => (
                    <div key={day.key} className="stats-forecast-col">
                      <span className="stats-forecast-count">{day.count}</span>
                      <div className="stats-forecast-track">
                        <span
                          className={day.count > 0 ? 'stats-forecast-bar has-activity' : 'stats-forecast-bar'}
                          style={{ height: `${day.count === 0 ? 4 : Math.max(12, (day.count / maxUpcoming) * 100)}%` }}
                        />
                      </div>
                      <span className="stats-forecast-day">{day.label}</span>
                    </div>
                  ))}
                </div>
              </section>
            )}
          </>
        )}

        <p className="stats-scope-note">
          Stats begin when review history was enabled. They include FSRS-effective grades, not fixed learning-step clicks.
        </p>
      </div>
    </>
  )
}

function Kpi({ label, displayLabel, value }: { label: string; displayLabel?: string; value: string }) {
  return (
    <div className="stats-kpi" aria-label={label}>
      <strong>{value}</strong>
      <span>{displayLabel ?? label}</span>
    </div>
  )
}

function percent(value: number): string {
  return `${Math.round(value * 100)}%`
}

function formatDay(timestamp: number): string {
  return new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}
