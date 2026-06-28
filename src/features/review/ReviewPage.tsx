import { useCallback, useEffect, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import type { Card, Grade, SchedulingState } from '../../domain/models'
import { getScheduler } from '../../domain/scheduler'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'
import { loadDueOverview, shuffle } from './dueOverview'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const [queue, setQueue] = useState<Card[] | null>(null)
  const [index, setIndex] = useState(0)
  const [revealed, setRevealed] = useState(false)
  const [nexts, setNexts] = useState<Record<Grade, SchedulingState> | null>(null)
  const [revealedAt, setRevealedAt] = useState(0)

  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const deckName = deckId ? (deck?.name ?? '') : 'All decks'
  const backTo = deckId ? `/decks/${deckId}` : '/'

  useEffect(() => {
    let active = true
    const now = Date.now()
    const load = deckId
      ? storage.dueCards(deckId, now)
      : loadDueOverview(storage, now).then((ov) => shuffle(ov.queue))
    load.then((cards) => {
      if (active) setQueue(cards)
    })
    return () => {
      active = false
    }
  }, [deckId, storage])

  const current = queue && index < queue.length ? queue[index] : null

  const reveal = useCallback(() => {
    if (!current) return
    const now = Date.now()
    setRevealed(true)
    setRevealedAt(now)
    setNexts(null)
    void getScheduler()
      .previewNextStates(current.scheduling, now)
      .then(setNexts)
  }, [current])

  const grade = useCallback(
    async (g: Grade) => {
      if (!current || !nexts) return
      await storage.updateCard(current.id, { scheduling: nexts[g] })
      setIndex((i) => i + 1)
      setRevealed(false)
      setNexts(null)
    },
    [current, nexts, storage],
  )

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!current) return
      if (!revealed) {
        if (e.code === 'Space' || e.key === 'Enter') {
          e.preventDefault()
          reveal()
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
  }, [current, revealed, reveal, grade])

  if (queue === null) return null

  if (queue.length === 0) {
    return (
      <div className="page-body">
        <div className="empty-state">
          <div className="ico">🌙</div>
          <h3>Nothing due</h3>
          <p>{deckId ? 'Nothing due in this deck right now.' : 'Nothing due across your decks right now.'}</p>
          <Link to={backTo} className="btn btn-ghost cta">
            {deckId ? 'Back to deck' : 'Back to Today'}
          </Link>
        </div>
      </div>
    )
  }

  if (current === null) {
    return (
      <div className="page-body">
        <div className="empty-state">
          <div className="ico">🎉</div>
          <h3>Review complete</h3>
          <p>
            {queue.length} card{queue.length === 1 ? '' : 's'} done. Nice work.
          </p>
          <Link to={backTo} className="btn btn-primary cta">
            {deckId ? 'Back to deck' : 'Back to Today'}
          </Link>
        </div>
      </div>
    )
  }

  const title = (
    <>
      <span className="review-pos">
        {index + 1} / {queue.length}
      </span>
      <span className="review-deck">{deckName}</span>
    </>
  )

  return (
    <>
      <PageHeader
        title={title}
        actions={
          <Link to={backTo} className="btn btn-ghost">
            End session
          </Link>
        }
      />
      <div className="review">
        {!revealed ? (
          <div className="review-stage">
            <div className="review-card">
              <div className="review-q">
                <MarkdownView source={current.front} />
              </div>
            </div>
            <button className="review-show" onClick={reveal}>
              Show answer <span className="kbd">space</span>
            </button>
          </div>
        ) : (
          <div className="review-stage reveal-enter">
            <div className="review-card revealed">
              <div className="review-q">
                <MarkdownView source={current.front} />
              </div>
              <hr className="review-rule" />
              <p className="answer-label">Answer</p>
              <div className="review-a">
                <MarkdownView source={current.back} />
              </div>
            </div>
            {nexts && <GradeButtons nexts={nexts} now={revealedAt} onGrade={grade} />}
          </div>
        )}
      </div>
    </>
  )
}
