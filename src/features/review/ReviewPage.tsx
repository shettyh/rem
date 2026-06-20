import { useCallback, useEffect, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import type { Card, Grade } from '../../domain/models'
import { scheduler } from '../../domain/scheduler'
import { useStorage } from '../../data/StorageContext'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  // The queue is a snapshot taken when the session starts, so cards graded into
  // the future don't disappear or reorder mid-session.
  const [queue, setQueue] = useState<Card[] | null>(null)
  const [index, setIndex] = useState(0)
  const [revealed, setRevealed] = useState(false)

  useEffect(() => {
    if (!deckId) return
    let active = true
    storage.dueCards(deckId, Date.now()).then((cards) => {
      if (active) setQueue(cards)
    })
    return () => {
      active = false
    }
  }, [deckId, storage])

  const current = queue && index < queue.length ? queue[index] : null

  const grade = useCallback(
    async (g: Grade) => {
      if (!current) return
      const next = scheduler.next(current.scheduling, g, Date.now())
      await storage.updateCard(current.id, { scheduling: next })
      setIndex((i) => i + 1)
      setRevealed(false)
    },
    [current, storage],
  )

  // Keyboard: Space/Enter reveals; 1–4 grade once revealed.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!current) return
      if (!revealed) {
        if (e.code === 'Space' || e.key === 'Enter') {
          e.preventDefault()
          setRevealed(true)
        }
        return
      }
      const byKey: Record<string, Grade> = { '1': 'again', '2': 'hard', '3': 'good', '4': 'easy' }
      const g = byKey[e.key]
      if (g) {
        e.preventDefault()
        void grade(g)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [current, revealed, grade])

  if (!deckId || queue === null) return null

  if (queue.length === 0) {
    return (
      <div className="stack">
        <p className="empty">Nothing due in this deck right now.</p>
        <BackToDeck deckId={deckId} />
      </div>
    )
  }

  if (current === null) {
    return (
      <div className="stack">
        <p className="empty">
          Review complete — {queue.length} card{queue.length === 1 ? '' : 's'} done. 🎉
        </p>
        <BackToDeck deckId={deckId} />
      </div>
    )
  }

  return (
    <div className="stack">
      <div className="row between">
        <span className="muted">
          {index + 1} / {queue.length}
        </span>
        <BackToDeck deckId={deckId} label="End session" />
      </div>

      <div className="review-card">
        <div className="review-side">
          <MarkdownView source={current.front} />
        </div>
        {revealed && (
          <>
            <hr className="divider" />
            <div className="review-side">
              <MarkdownView source={current.back} />
            </div>
          </>
        )}
      </div>

      {revealed ? (
        <GradeButtons scheduling={current.scheduling} now={Date.now()} onGrade={grade} />
      ) : (
        <button className="btn btn-primary" onClick={() => setRevealed(true)}>
          Show answer <span className="grade-key">space</span>
        </button>
      )}
    </div>
  )
}

function BackToDeck({ deckId, label = 'Back to deck' }: { deckId: string; label?: string }) {
  return (
    <Link to={`/decks/${deckId}`} className="btn btn-ghost">
      {label}
    </Link>
  )
}
