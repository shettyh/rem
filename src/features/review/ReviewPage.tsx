import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import type { Card, Grade } from '../../domain/models'
import { getScheduler } from '../../domain/scheduler'
import { useStorage } from '../../data/StorageContext'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const [queue, setQueue] = useState<Card[] | null>(null)
  const [index, setIndex] = useState(0)
  const [revealed, setRevealed] = useState(false)

  const frontRef = useRef<HTMLDivElement>(null)
  const backRef = useRef<HTMLDivElement>(null)
  const [cardH, setCardH] = useState(220)

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

  // Size the flip card to the taller face, capped to 65vh; the answer scrolls beyond that.
  useLayoutEffect(() => {
    if (!current) return
    const measure = () => {
      const f = frontRef.current?.scrollHeight ?? 0
      const b = backRef.current?.scrollHeight ?? 0
      const cap = Math.round(window.innerHeight * 0.65)
      setCardH(Math.max(160, Math.min(cap, Math.max(f, b))))
    }
    measure()
    window.addEventListener('resize', measure)
    return () => window.removeEventListener('resize', measure)
  }, [current])

  const grade = useCallback(
    async (g: Grade) => {
      if (!current) return
      const next = getScheduler(current.scheduling.kind).next(current.scheduling, g, Date.now())
      await storage.updateCard(current.id, { scheduling: next })
      setIndex((i) => i + 1)
      setRevealed(false)
    },
    [current, storage],
  )

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
        <div className="empty-state">
          <div className="ico">🌙</div>
          <h3>Nothing due</h3>
          <p>Nothing due in this deck right now.</p>
          <BackToDeck deckId={deckId} className="btn btn-ghost cta" />
        </div>
      </div>
    )
  }

  if (current === null) {
    return (
      <div className="stack">
        <div className="empty-state">
          <div className="ico">🎉</div>
          <h3>Review complete</h3>
          <p>
            {queue.length} card{queue.length === 1 ? '' : 's'} done. Nice work.
          </p>
          <BackToDeck deckId={deckId} className="btn btn-primary cta" />
        </div>
      </div>
    )
  }

  return (
    <div className="stack">
      <div className="row between">
        <span className="muted">
          {index + 1} / {queue.length}
        </span>
        <BackToDeck deckId={deckId} label="End session" className="btn btn-ghost" />
      </div>

      <div className="flip" style={{ height: `${cardH}px` }}>
        <div className="flip-inner" data-testid="flip" data-revealed={revealed}>
          <div className="face face-front">
            <div className="face-inner" ref={frontRef}>
              <div className="review-side">
                <MarkdownView source={current.front} />
              </div>
            </div>
            <button className="btn btn-primary show-btn" onClick={() => setRevealed(true)}>
              Show answer <span className="kbd">space</span>
            </button>
          </div>
          <div className="face face-back">
            <div className="scroll" ref={backRef}>
              <p className="answer-label">Answer</p>
              <div className="review-side">
                <MarkdownView source={current.back} />
              </div>
            </div>
          </div>
        </div>
      </div>

      {revealed && (
        <GradeButtons scheduling={current.scheduling} now={Date.now()} onGrade={grade} />
      )}
    </div>
  )
}

function BackToDeck({
  deckId,
  label = 'Back to deck',
  className = 'btn btn-ghost',
}: {
  deckId: string
  label?: string
  className?: string
}) {
  return (
    <Link to={`/decks/${deckId}`} className={className}>
      {label}
    </Link>
  )
}
