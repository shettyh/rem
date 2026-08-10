import type { Grade, SchedulingState } from '../../domain/models'

const GRADES: { grade: Grade; label: string; key: string }[] = [
  { grade: 'again', label: 'Again', key: '1' },
  { grade: 'hard', label: 'Hard', key: '2' },
  { grade: 'good', label: 'Good', key: '3' },
  { grade: 'easy', label: 'Easy', key: '4' },
]

/** Human-readable interval from a millisecond delta: 1m / 10m / 2h / 6d / 2mo / 1y. */
function formatInterval(ms: number): string {
  const min = Math.round(ms / 60_000)
  if (min < 60) return `${Math.max(1, min)}m`
  const hrs = Math.round(ms / 3_600_000)
  if (hrs < 24) return `${hrs}h`
  const days = Math.round(ms / 86_400_000)
  if (days < 30) return `${days}d`
  if (days < 365) return `${Math.round(days / 30)}mo`
  return `${Math.round(days / 365)}y`
}

/** The four grading buttons, each previewing the interval it would schedule,
 *  from the pre-computed next-states. */
export function GradeButtons({
  nexts,
  now,
  onGrade,
}: {
  nexts: Record<Grade, SchedulingState>
  now: number
  onGrade: (grade: Grade) => void
}) {
  return (
    <div className="grade-row" role="group" aria-label="Grade answer">
      {GRADES.map(({ grade, label, key }) => (
        <button
          key={grade}
          type="button"
          className={`grade grade-${grade}`}
          onClick={() => onGrade(grade)}
        >
          <span className="grade-key">{key}</span>
          <span className="grade-label">{label}</span>
          <span className="grade-hint">{formatInterval(nexts[grade].due - now)}</span>
        </button>
      ))}
    </div>
  )
}
