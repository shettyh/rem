import type { Grade, SchedulingState } from '../../domain/models'
import { scheduler } from '../../domain/scheduler'

const GRADES: { grade: Grade; label: string; key: string }[] = [
  { grade: 'again', label: 'Again', key: '1' },
  { grade: 'hard', label: 'Hard', key: '2' },
  { grade: 'good', label: 'Good', key: '3' },
  { grade: 'easy', label: 'Easy', key: '4' },
]

/** Human-readable interval, e.g. 1d / 6d / 2mo / 1y. */
function formatInterval(days: number): string {
  if (days >= 365) return `${Math.round(days / 365)}y`
  if (days >= 30) return `${Math.round(days / 30)}mo`
  return `${days}d`
}

/** The four grading buttons, each previewing the interval it would schedule. */
export function GradeButtons({
  scheduling,
  now,
  onGrade,
}: {
  scheduling: SchedulingState
  now: number
  onGrade: (grade: Grade) => void
}) {
  return (
    <div className="grade-row">
      {GRADES.map(({ grade, label, key }) => (
        <button
          key={grade}
          className={`btn grade grade-${grade}`}
          onClick={() => onGrade(grade)}
        >
          <span className="grade-label">{label}</span>
          <span className="grade-hint">{formatInterval(scheduler.next(scheduling, grade, now).intervalDays)}</span>
          <span className="kbd">{key}</span>
        </button>
      ))}
    </div>
  )
}
