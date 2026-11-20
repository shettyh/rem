import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import type { Grade, StudyRequest, StudyView } from '../../domain/models'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import { EmptyIcon } from '../../ui/EmptyIcon'
import { PageHeader } from '../../ui/PageHeader'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'
import {
  customStudyPreset,
  parseCustomStudyRequest,
} from './customStudy'

function formatDuration(ms: number): string {
  const totalSec = Math.max(1, Math.round(ms / 1000))
  if (totalSec < 60) return `${totalSec}s`
  const min = Math.floor(totalSec / 60)
  const sec = totalSec % 60
  return sec === 0 ? `${min}m` : `${min}m ${sec}s`
}

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
  const location = useLocation()
  const [searchParams] = useSearchParams()
  const customRequest = deckId ? parseCustomStudyRequest(searchParams) : null
  const customMode = customRequest?.mode ?? null
  const isPreview = customMode === 'preview-new'

  const sessionIdRef = useRef<string | null>(null)
  const gradingRef = useRef(false)
  const sessionStartRef = useRef(Date.now())
  const [view, setView] = useState<StudyView | null>(null)
  const [ready, setReady] = useState(false)
  const [revealedAt, setRevealedAt] = useState(0)
  const [schedError, setSchedError] = useState(false)
  const [conflictNotice, setConflictNotice] = useState(false)
  const [sessionStats, setSessionStats] = useState({ total: 0, recalled: 0, elapsedMs: 0 })

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const deckName = deckId ? (deck?.name ?? 'Deck') : 'All decks'
  const customTitle = customMode ? customStudyPreset(customMode).title : null
  const backTo = customMode && deckId ? `/decks/${deckId}/options` : deckId ? `/decks/${deckId}` : '/'
  const backLabel = customMode ? 'Back to options' : deckId ? 'Back to deck' : 'Back to Today'

  useEffect(() => {
    let active = true
    let startedId: string | null = null
    sessionStartRef.current = Date.now()
    setSessionStats({ total: 0, recalled: 0, elapsedMs: 0 })
    const request: StudyRequest = {
      deckId: deckId ?? null,
      custom: customRequest,
    }
    void storage.startStudy(request).then((started) => {
      startedId = started.sessionId
      if (!active) {
        void storage.endStudy(started.sessionId)
        return
      }
      sessionIdRef.current = started.sessionId
      setView(started.view)
      setReady(true)
    })
    return () => {
      active = false
      if (startedId) void storage.endStudy(startedId)
      if (sessionIdRef.current === startedId) sessionIdRef.current = null
    }
  }, [deckId, storage, customMode, customRequest?.amount])

  const reveal = useCallback(() => {
    const sessionId = sessionIdRef.current
    if (!sessionId || !view?.current || (view.revealed && !schedError)) return
    const now = Date.now()
    setRevealedAt(now)
    void storage.revealStudy(sessionId)
      .then((nextView) => {
        setView(nextView)
        setSchedError(false)
      })
      .catch((err: unknown) => {
        console.error('study reveal failed', err)
        setSchedError(true)
      })
  }, [storage, view, schedError])

  const advancePreview = useCallback(async () => {
    const sessionId = sessionIdRef.current
    if (!isPreview || !sessionId || gradingRef.current) return
    gradingRef.current = true
    try {
      const next = await storage.advanceStudyPreview(sessionId)
      setView(next)
      if (next.current === null) {
        setSessionStats((prev) => ({ ...prev, elapsedMs: Date.now() - sessionStartRef.current }))
      }
    } finally {
      gradingRef.current = false
    }
  }, [isPreview, storage])

  const grade = useCallback(
    async (grade: Grade) => {
      const sessionId = sessionIdRef.current
      if (!sessionId || !view?.current || !view.nextStates || gradingRef.current) return
      gradingRef.current = true
      try {
        const outcome = await storage.gradeStudy(sessionId, grade)
        setView(outcome.view)
        setSchedError(false)
        setConflictNotice(outcome.status === 'conflict')
        if (outcome.status !== 'conflict') {
          setSessionStats((prev) => ({
            total: prev.total + 1,
            recalled: prev.recalled + (grade === 'again' ? 0 : 1),
            elapsedMs: Date.now() - sessionStartRef.current,
          }))
        }
      } finally {
        gradingRef.current = false
      }
    },
    [storage, view],
  )

  const editCurrent = useCallback(() => {
    const card = view?.current
    if (!card) return
    navigate(`/decks/${card.deckId}/cards/${card.id}/edit`, {
      state: { returnTo: `${location.pathname}${location.search}` },
    })
  }, [view?.current, navigate, location.pathname, location.search])

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement | null)?.tagName ?? ''
      if (tag === 'INPUT' || tag === 'TEXTAREA') return
      if (!view?.current) return
      if (e.key === 'e' || e.key === 'E') {
        e.preventDefault()
        editCurrent()
        return
      }
      if (!view.revealed && !schedError) {
        if (e.code === 'Space' || e.key === 'Enter') {
          e.preventDefault()
          reveal()
        }
        return
      }
      if (isPreview && (e.code === 'Space' || e.key === 'Enter')) {
        e.preventDefault()
        void advancePreview()
        return
      }
      const byKey: Record<string, Grade> = { '1': 'again', '2': 'hard', '3': 'good', '4': 'easy' }
      const keyGrade = byKey[e.key]
      if (keyGrade) {
        e.preventDefault()
        void grade(keyGrade)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [view, isPreview, schedError, reveal, advancePreview, grade, editCurrent])

  if (!ready || !view) return null

  const current = view.current
  const leechMessage = view.notice === 'suspend'
    ? 'Leech suspended. Edit the card to restore it.'
    : view.notice === 'tag'
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
    if (conflictNotice && view.reviewed === 0) {
      return (
        <>
          <PageHeader title={terminalTitle} actions={terminalAction} />
          <div className="review-terminal">
            <div className="empty-state">
              <EmptyIcon kind="skip" />
              <h3>Card changed</h3>
              <p role="alert">The stale card was skipped without recording a review.</p>
              <Link to={backTo} className="btn btn-ghost cta">{backLabel}</Link>
            </div>
          </div>
        </>
      )
    }
    if (view.reviewed === 0) {
      return (
        <>
          <PageHeader title={terminalTitle} actions={terminalAction} />
          <div className="review-terminal">
            <div className="empty-state">
              <EmptyIcon kind="rest" />
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
    const recallPct = sessionStats.total > 0
      ? Math.round((sessionStats.recalled / sessionStats.total) * 100)
      : null
    return (
      <>
        <PageHeader title={terminalTitle} actions={terminalAction} />
        <div className="review-terminal">
          <div className="empty-state review-complete-card">
            <EmptyIcon kind="done" tone="success" />
            <h3>{isPreview ? 'Preview complete' : 'Review complete'}</h3>
            <p>
              {isPreview
                ? `${view.reviewed} card${view.reviewed === 1 ? '' : 's'} previewed.`
                : `${view.reviewed} review${view.reviewed === 1 ? '' : 's'} done. Nice work.`}
            </p>
            <div className="session-summary-strip" aria-label="Session summary">
              <div className="session-stat">
                <strong>{view.reviewed}</strong>
                <span>{isPreview ? 'Previewed' : 'Reviewed'}</span>
              </div>
              {!isPreview && recallPct !== null && (
                <div className="session-stat">
                  <strong>{recallPct}%</strong>
                  <span>Recalled</span>
                </div>
              )}
              <div className="session-stat">
                <strong>{formatDuration(sessionStats.elapsedMs)}</strong>
                <span>Time</span>
              </div>
            </div>
            {leechMessage && <p className="review-complete-note" role="status">{leechMessage}</p>}
            {conflictNotice && <p className="review-complete-note" role="alert">A changed card was skipped instead of grading stale content.</p>}
            <Link to={backTo} className="btn btn-primary cta">{backLabel}</Link>
          </div>
        </div>
      </>
    )
  }

  const displayedRevealed = view.revealed || schedError
  const total = view.reviewed + view.remaining
  const position = view.reviewed + 1
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
          <>
            <button
              type="button"
              className="btn btn-ghost"
              title="Edit card (E)"
              onClick={editCurrent}
            >
              Edit
            </button>
            <Link to={backTo} className="btn btn-ghost">
              End session
            </Link>
          </>
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
      <div className={`review${displayedRevealed ? ' is-revealed' : ''}`}>
        <div className="review-scroll">
          <div className={`review-stage${displayedRevealed ? ' reveal-enter' : ''}`}>
            {leechMessage && <p className="review-notice" role="status">{leechMessage}</p>}
            {conflictNotice && (
              <p className="review-notice" role="alert">
                A changed card was skipped instead of grading stale content.
              </p>
            )}
            {!displayedRevealed ? (
              <div className="review-card">
                <section className="review-question" aria-label="Question">
                  <p className="review-label">Question</p>
                  <div className="review-q">
                    <MarkdownView source={current.front} />
                  </div>
                </section>
              </div>
            ) : (
              <div className="review-card revealed">
                <section className="review-question" aria-label="Question">
                  <p className="review-label">Question</p>
                  <div className="review-q">
                    <MarkdownView source={current.front} />
                  </div>
                </section>
                <hr className="review-rule" />
                <section className="review-answer" aria-label="Answer">
                  <p className="review-label">Answer</p>
                  <div className="review-a">
                    <MarkdownView source={current.back} />
                  </div>
                </section>
              </div>
            )}
          </div>
        </div>
        <div className="review-dock">
          <div className="review-actions">
            {!displayedRevealed ? (
              <button className="btn btn-primary review-show" onClick={reveal}>
                Show answer <span className="kbd">space</span>
              </button>
            ) : isPreview ? (
              <button className="btn btn-primary review-show" onClick={advancePreview}>
                Next card <span className="kbd">space</span>
              </button>
            ) : view.nextStates ? (
              <GradeButtons nexts={view.nextStates} now={revealedAt} onGrade={grade} />
            ) : null}
            {!isPreview && schedError && !view.nextStates && (
              <div className="review-schedule-error" role="alert">
                <p>Couldn&#39;t schedule this card.</p>
                <button className="btn btn-ghost" onClick={reveal}>
                  Retry
                </button>
              </div>
            )}
          </div>
        </div>
      </div>
    </>
  )
}
