import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import type { DeckSettings, FSRSState, Grade, LeechAction } from '../../domain/models'
import { nextStates } from '../../domain/scheduler/reviewScheduler'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import type { CardPatch } from '../../data/Storage'
import { PageHeader } from '../../ui/PageHeader'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'
import { loadDueOverview, shuffle } from './dueOverview'
import { ReviewSession, buildSessionCards, type SessionCard } from './session'
import { localDay } from './day'
import { leechEffect } from './leech'
import {
  customStudyPreset,
  parseCustomStudyRequest,
  selectCustomStudyCards,
} from './customStudy'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const [searchParams] = useSearchParams()
  const customRequest = deckId ? parseCustomStudyRequest(searchParams) : null
  const customMode = customRequest?.mode ?? null
  const customAmount = customRequest?.amount ?? null
  const isPreview = customMode === 'preview-new'

  const sessionRef = useRef<ReviewSession | null>(null)
  const gradingRef = useRef(false)
  const [current, setCurrent] = useState<SessionCard | null>(null)
  const [ready, setReady] = useState(false)
  const [revealed, setRevealed] = useState(false)
  const [nexts, setNexts] = useState<Record<Grade, FSRSState> | null>(null)
  const [revealedAt, setRevealedAt] = useState(0)
  const [schedError, setSchedError] = useState(false)
  const [leechNotice, setLeechNotice] = useState<LeechAction | null>(null)

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const deckName = deckId ? (deck?.name ?? 'Deck') : 'All decks'
  const customTitle = customMode ? customStudyPreset(customMode).title : null
  const backTo = customMode && deckId ? `/decks/${deckId}/options` : deckId ? `/decks/${deckId}` : '/'
  const backLabel = customMode ? 'Back to options' : deckId ? 'Back to deck' : 'Back to Today'

  useEffect(() => {
    let active = true
    const now = Date.now()
    async function build(): Promise<SessionCard[]> {
      if (deckId) {
        const d = await storage.getDeck(deckId)
        if (!d) return []
        const stat = await storage.getDailyStat(deckId, localDay(now))
        const newSlots = d.settings.newPerDay - stat.newIntroduced
        if (customMode && customAmount !== null) {
          const cards = await storage.listCards(deckId)
          return selectCustomStudyCards(cards, { mode: customMode, amount: customAmount }, now, {
            insertionOrder: d.settings.insertionOrder,
            normalNewSlots: newSlots,
          }).map((card) => ({ card, settings: d.settings, forceDue: true }))
        }
        const due = await storage.dueCards(deckId, now)
        const caps = {
          newSlots,
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
  }, [deckId, storage, customMode, customAmount])

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
    if (!isPreview) fetchNexts(current, now)
  }, [current, revealed, isPreview, fetchNexts])

  const advancePreview = useCallback(() => {
    const session = sessionRef.current
    if (!isPreview || !session) return
    session.complete()
    setCurrent(session.next(Date.now()))
    setRevealed(false)
  }, [isPreview])

  const grade = useCallback(
    async (g: Grade) => {
      const session = sessionRef.current
      if (!current || !nexts || !session || gradingRef.current) return
      gradingRef.current = true
      try {
        const gradedAt = Date.now()
        const preState = current.card.scheduling.state
        const next = nexts[g]
        const effect = leechEffect(current.card, current.settings, g, next)
        const patch: CardPatch = { scheduling: next }
        if (effect) {
          patch.tags = effect.tags
          patch.suspended = effect.suspended
        }
        if (g === 'again') patch.lastAgainAt = gradedAt
        const daily = preState === 0
          ? { day: localDay(gradedAt), field: 'newIntroduced' as const }
          : preState === 2
            ? { day: localDay(gradedAt), field: 'reviewsDone' as const }
            : undefined
        await storage.commitReview({
          cardId: current.card.id,
          deckId: current.card.deckId,
          patch,
          reviewedAt: gradedAt,
          fsrsGrade: next.reps > current.card.scheduling.reps ? g : undefined,
          daily,
        })
        session.grade(gradedAt, next, { requeue: effect?.suspended !== true })
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
      if (isPreview && (e.code === 'Space' || e.key === 'Enter')) {
        e.preventDefault()
        advancePreview()
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
  }, [current, revealed, isPreview, reveal, advancePreview, grade])

  if (!ready) return null

  const leechMessage = leechNotice === 'suspend'
    ? 'Leech suspended. Edit the card to restore it.'
    : leechNotice === 'tag'
      ? 'Leech tagged.'
      : null
  const contextLabel = customTitle ? `${deckName} · ${customTitle}` : deckName
  const terminalTitle = (
    <>
      <span className="header-title-text">{isPreview ? 'Preview' : 'Review'}</span>
      <span className="review-deck">{contextLabel}</span>
    </>
  )
  const terminalAction = <Link to={backTo} className="btn btn-ghost">Close</Link>

  if (current === null) {
    const reviewed = sessionRef.current?.reviewed ?? 0
    if (reviewed === 0) {
      return (
        <>
          <PageHeader title={terminalTitle} actions={terminalAction} />
          <div className="review-terminal">
            <div className="empty-state">
              <div className="ico" aria-hidden="true">REST</div>
              <h3>{customMode ? 'No matching cards' : 'Nothing due'}</h3>
              <p>
                {customMode
                  ? `No cards match ${customTitle?.toLowerCase()} right now.`
                  : deckId
                    ? 'Nothing due in this deck right now.'
                    : 'Nothing due across your decks right now.'}
              </p>
              <Link to={backTo} className="btn btn-ghost cta">{backLabel}</Link>
            </div>
          </div>
        </>
      )
    }
    return (
      <>
        <PageHeader title={terminalTitle} actions={terminalAction} />
        <div className="review-terminal">
          <div className="empty-state">
            <div className="ico" aria-hidden="true">DONE</div>
            <h3>{isPreview ? 'Preview complete' : 'Review complete'}</h3>
            <p>
              {isPreview
                ? `${reviewed} card${reviewed === 1 ? '' : 's'} previewed.`
                : `${reviewed} review${reviewed === 1 ? '' : 's'} done. Nice work.`}
            </p>
            {leechMessage && <p role="status">{leechMessage}</p>}
            <Link to={backTo} className="btn btn-primary cta">{backLabel}</Link>
          </div>
        </div>
      </>
    )
  }

  const reviewed = sessionRef.current?.reviewed ?? 0
  const total = reviewed + (sessionRef.current?.remaining ?? 0)
  const position = reviewed + 1
  const title = (
    <>
      <span className="review-pos">
        {position} / {total}
      </span>
      <span className="review-deck">{contextLabel}</span>
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
      <div
        className="review-progress"
        role="progressbar"
        aria-label="Review progress"
        aria-valuemin={1}
        aria-valuemax={total}
        aria-valuenow={position}
        aria-valuetext={`Card ${position} of ${total}`}
      >
        <span style={{ width: `${Math.min(100, (position / total) * 100)}%` }} />
      </div>
      <div className="review">
        {leechMessage && <p className="review-notice" role="status">{leechMessage}</p>}
        {!revealed ? (
          <div className="review-stage">
            <div className="review-card">
              <div className="review-q">
                <MarkdownView source={current.card.front} />
              </div>
            </div>
            <div className="review-actions">
              <button className="btn btn-primary review-show" onClick={reveal}>
                Show answer <span className="kbd">space</span>
              </button>
            </div>
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
            <div className="review-actions">
              {isPreview ? (
                <button className="btn btn-primary review-show" onClick={advancePreview}>
                  Next card <span className="kbd">space</span>
                </button>
              ) : nexts ? (
                <GradeButtons nexts={nexts} now={revealedAt} onGrade={grade} />
              ) : null}
              {!isPreview && schedError && !nexts && (
                <div className="review-schedule-error" role="alert">
                  <p>Couldn&#39;t schedule this card.</p>
                  <button className="btn btn-ghost" onClick={() => fetchNexts(current, revealedAt)}>
                    Retry
                  </button>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </>
  )
}
