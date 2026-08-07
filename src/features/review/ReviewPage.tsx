import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import type { DeckSettings, FSRSState, Grade, LeechAction } from '../../domain/models'
import { nextStates } from '../../domain/scheduler/reviewScheduler'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'
import { loadDueOverview, shuffle } from './dueOverview'
import { ReviewSession, buildSessionCards, type SessionCard } from './session'
import { localDay } from './day'
import { leechEffect } from './leech'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const sessionRef = useRef<ReviewSession | null>(null)
  const gradingRef = useRef(false)
  const [current, setCurrent] = useState<SessionCard | null>(null)
  const [ready, setReady] = useState(false)
  const [revealed, setRevealed] = useState(false)
  const [nexts, setNexts] = useState<Record<Grade, FSRSState> | null>(null)
  const [revealedAt, setRevealedAt] = useState(0)
  const [schedError, setSchedError] = useState(false)
  const [leechNotice, setLeechNotice] = useState<LeechAction | null>(null)

  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const deckName = deckId ? (deck?.name ?? '') : 'All decks'
  const backTo = deckId ? `/decks/${deckId}` : '/'

  useEffect(() => {
    let active = true
    const now = Date.now()
    async function build(): Promise<SessionCard[]> {
      if (deckId) {
        const d = await storage.getDeck(deckId)
        if (!d) return []
        const due = await storage.dueCards(deckId, now)
        const stat = await storage.getDailyStat(deckId, localDay(now))
        const caps = {
          newSlots: d.settings.newPerDay - stat.newIntroduced,
          reviewSlots: d.settings.maxReviews - stat.reviewsDone,
        }
        const cards = due.map((card) => ({ card, settings: d.settings }))
        return buildSessionCards(cards, d.settings.insertionOrder, caps)
      }
      const ov = await loadDueOverview(storage, now)
      const settingsById = new Map<string, DeckSettings>(ov.decks.map((o) => [o.deck.id, o.deck.settings]))
      return shuffle(ov.queue).map((card) => ({ card, settings: settingsById.get(card.deckId)! }))
    }
    void build().then((cards) => {
      if (!active) return
      const session = new ReviewSession(cards)
      sessionRef.current = session
      setCurrent(session.next(Date.now()))
      setReady(true)
    })
    return () => {
      active = false
    }
  }, [deckId, storage])

  const fetchNexts = useCallback((sc: SessionCard, now: number) => {
    setSchedError(false)
    setNexts(null)
    void nextStates(sc.card.scheduling, sc.settings, now)
      .then(setNexts)
      .catch((err: unknown) => {
        console.error('nextStates failed', err)
        setSchedError(true)
      })
  }, [])

  const reveal = useCallback(() => {
    if (!current || revealed) return
    const now = Date.now()
    setRevealed(true)
    setRevealedAt(now)
    fetchNexts(current, now)
  }, [current, revealed, fetchNexts])

  const grade = useCallback(
    async (g: Grade) => {
      const session = sessionRef.current
      if (!current || !nexts || !session || gradingRef.current) return
      gradingRef.current = true
      try {
        const preState = current.card.scheduling.state
        const next = nexts[g]
        const effect = leechEffect(current.card, current.settings, g, next)
        await storage.updateCard(current.card.id, effect
          ? { scheduling: next, tags: effect.tags, suspended: effect.suspended }
          : { scheduling: next })
        const day = localDay(Date.now())
        if (preState === 0) await storage.bumpDailyStat(current.card.deckId, day, 'newIntroduced')
        else if (preState === 2) await storage.bumpDailyStat(current.card.deckId, day, 'reviewsDone')
        session.grade(Date.now(), next, { requeue: effect?.suspended !== true })
        setCurrent(session.next(Date.now()))
        setRevealed(false)
        setNexts(null)
        setSchedError(false)
        setLeechNotice(effect?.action ?? null)
      } finally {
        gradingRef.current = false
      }
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

  if (!ready) return null

  const leechMessage = leechNotice === 'suspend'
    ? 'Leech suspended. Edit the card to restore it.'
    : leechNotice === 'tag'
      ? 'Leech tagged.'
      : null

  if (current === null) {
    const reviewed = sessionRef.current?.reviewed ?? 0
    if (reviewed === 0) {
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
    return (
      <div className="page-body">
        <div className="empty-state">
          <div className="ico">🎉</div>
          <h3>Review complete</h3>
          <p>
            {reviewed} review{reviewed === 1 ? '' : 's'} done. Nice work.
          </p>
          {leechMessage && <p role="status">{leechMessage}</p>}
          <Link to={backTo} className="btn btn-primary cta">
            {deckId ? 'Back to deck' : 'Back to Today'}
          </Link>
        </div>
      </div>
    )
  }

  const reviewed = sessionRef.current?.reviewed ?? 0
  const total = reviewed + (sessionRef.current?.remaining ?? 0)
  const title = (
    <>
      <span className="review-pos">
        {reviewed + 1} / {total}
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
        {leechMessage && <p className="review-notice" role="status">{leechMessage}</p>}
        {!revealed ? (
          <div className="review-stage">
            <div className="review-card">
              <div className="review-q">
                <MarkdownView source={current.card.front} />
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
                <MarkdownView source={current.card.front} />
              </div>
              <hr className="review-rule" />
              <p className="answer-label">Answer</p>
              <div className="review-a">
                <MarkdownView source={current.card.back} />
              </div>
            </div>
            {nexts && <GradeButtons nexts={nexts} now={revealedAt} onGrade={grade} />}
            {schedError && !nexts && (
              <div className="empty-state">
                <p>Couldn&#39;t schedule this card.</p>
                <button className="btn btn-ghost" onClick={() => fetchNexts(current, revealedAt)}>
                  Retry
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </>
  )
}
