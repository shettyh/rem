import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import type { Grade, StudyRequest, StudyView } from '../../domain/models'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import { EmptyIcon } from '../../ui/EmptyIcon'
import { PageHeader } from '../../ui/PageHeader'
import { MarkdownView } from '../cards/MarkdownView'
import { hasImageOcclusion } from '../cards/occlusion'
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

function formatTimer(seconds: number): string {
  const clamped = Math.max(0, Math.floor(seconds))
  const min = Math.floor(clamped / 60)
  const sec = clamped % 60
  return `${min}:${sec.toString().padStart(2, '0')}`
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
  const inFlightRef = useRef(false)
  const opGenRef = useRef(0)
  const sessionStartRef = useRef(Date.now())
  const lastGradedGradeRef = useRef<Grade | null>(null)
  const [view, setView] = useState<StudyView | null>(null)
  const [ready, setReady] = useState(false)
  const [startError, setStartError] = useState<string | null>(null)
  const [revealedAt, setRevealedAt] = useState(0)
  const [cardElapsedSec, setCardElapsedSec] = useState(0)
  const [timerAttempt, setTimerAttempt] = useState(0)
  const [schedError, setSchedError] = useState(false)
  const [conflictNotice, setConflictNotice] = useState(false)
  const [actionNotice, setActionNotice] = useState<string | null>(null)
  const [sessionStats, setSessionStats] = useState({ total: 0, recalled: 0, elapsedMs: 0 })

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const deckName = deckId ? (deck?.name ?? 'Deck') : 'All decks'
  const customTitle = customMode ? customStudyPreset(customMode).title : null
  const backTo = customMode && deckId ? `/decks/${deckId}/options` : deckId ? `/decks/${deckId}` : '/'
  const backLabel = customMode ? 'Back to options' : deckId ? 'Back to deck' : 'Back to Today'

  useEffect(() => {
    let active = true
    let startedId: string | null = null
    sessionIdRef.current = null
    inFlightRef.current = false
    opGenRef.current += 1
    sessionStartRef.current = Date.now()
    lastGradedGradeRef.current = null
    setReady(false)
    setView(null)
    setStartError(null)
    setSchedError(false)
    setConflictNotice(false)
    setActionNotice(null)
    setSessionStats({ total: 0, recalled: 0, elapsedMs: 0 })
    const request: StudyRequest = {
      deckId: deckId ?? null,
      custom: customRequest,
    }
    void storage
      .startStudy(request)
      .then((started) => {
        startedId = started.sessionId
        if (!active) {
          void storage.endStudy(started.sessionId)
          return
        }
        sessionIdRef.current = started.sessionId
        setView(started.view)
        setTimerAttempt((n) => n + 1)
        setReady(true)
      })
      .catch((err: unknown) => {
        if (!active) return
        console.error('study start failed', err)
        setStartError(err instanceof Error ? err.message : 'Could not start study session.')
        setReady(true)
      })
    return () => {
      active = false
      opGenRef.current += 1
      inFlightRef.current = false
      if (startedId) void storage.endStudy(startedId)
      if (sessionIdRef.current === startedId) sessionIdRef.current = null
    }
  }, [deckId, storage, customMode, customRequest?.amount])

  const reveal = useCallback(() => {
    const sessionId = sessionIdRef.current
    if (!sessionId || !view?.current || (view.revealed && !schedError) || inFlightRef.current) return
    inFlightRef.current = true
    const gen = ++opGenRef.current
    const now = Date.now()
    setRevealedAt(now)
    setActionNotice(null)
    void storage
      .revealStudy(sessionId)
      .then((nextView) => {
        if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
        setView(nextView)
        setSchedError(false)
      })
      .catch((err: unknown) => {
        if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
        console.error('study reveal failed', err)
        setSchedError(true)
      })
      .finally(() => {
        if (opGenRef.current === gen) inFlightRef.current = false
      })
  }, [storage, view, schedError])

  const advancePreview = useCallback(async () => {
    const sessionId = sessionIdRef.current
    if (!isPreview || !sessionId || inFlightRef.current) return
    inFlightRef.current = true
    const gen = ++opGenRef.current
    try {
      const next = await storage.advanceStudyPreview(sessionId)
      if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
      setView(next)
      setTimerAttempt((n) => n + 1)
      setActionNotice(null)
      if (next.current === null) {
        setSessionStats((prev) => ({ ...prev, elapsedMs: Date.now() - sessionStartRef.current }))
      }
    } catch (err: unknown) {
      if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
      console.error('advance preview failed', err)
      setActionNotice('Could not advance preview.')
    } finally {
      if (opGenRef.current === gen) inFlightRef.current = false
    }
  }, [isPreview, storage])

  const grade = useCallback(
    async (grade: Grade) => {
      const sessionId = sessionIdRef.current
      if (!sessionId || !view?.current || !view.nextStates || inFlightRef.current) return
      inFlightRef.current = true
      const gen = ++opGenRef.current
      try {
        const outcome = await storage.gradeStudy(sessionId, grade)
        if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
        setView(outcome.view)
        setTimerAttempt((n) => n + 1)
        setSchedError(false)
        setActionNotice(null)
        setConflictNotice(outcome.status === 'conflict')
        if (outcome.status !== 'conflict') {
          lastGradedGradeRef.current = grade
          setSessionStats((prev) => ({
            total: prev.total + 1,
            recalled: prev.recalled + (grade === 'again' ? 0 : 1),
            elapsedMs: Date.now() - sessionStartRef.current,
          }))
        }
      } catch (err: unknown) {
        if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
        console.error('study grade failed', err)
        setActionNotice('Could not record grade.')
      } finally {
        if (opGenRef.current === gen) inFlightRef.current = false
      }
    },
    [storage, view],
  )

  const undoLastGrade = useCallback(async () => {
    const sessionId = sessionIdRef.current
    if (!sessionId || !view?.canUndo || inFlightRef.current) return
    inFlightRef.current = true
    const gen = ++opGenRef.current
    try {
      const nextView = await storage.undoStudyGrade(sessionId)
      if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
      const undoneGrade = lastGradedGradeRef.current
      lastGradedGradeRef.current = null
      setView(nextView)
      setTimerAttempt((n) => n + 1)
      setSchedError(false)
      setConflictNotice(false)
      setActionNotice('Last grade undone.')
      if (undoneGrade) {
        setSessionStats((prev) => ({
          total: Math.max(0, prev.total - 1),
          recalled: Math.max(0, prev.recalled - (undoneGrade === 'again' ? 0 : 1)),
          elapsedMs: Date.now() - sessionStartRef.current,
        }))
      }
    } catch (err: unknown) {
      if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
      console.error('study undo failed', err)
      const msg = err instanceof Error ? err.message : String(err)
      if (/conflict|not found/i.test(msg)) {
        lastGradedGradeRef.current = null
        setView((prev) => (prev ? { ...prev, canUndo: false } : prev))
      }
      setActionNotice('Could not undo last grade.')
    } finally {
      if (opGenRef.current === gen) inFlightRef.current = false
    }
  }, [storage, view?.canUndo])

  const suspendCurrent = useCallback(async () => {
    const sessionId = sessionIdRef.current
    if (!sessionId || !view?.current || isPreview || inFlightRef.current) return
    inFlightRef.current = true
    const gen = ++opGenRef.current
    try {
      const nextView = await storage.suspendStudyCard(sessionId)
      if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
      lastGradedGradeRef.current = null
      setView(nextView)
      setTimerAttempt((n) => n + 1)
      setSchedError(false)
      setConflictNotice(false)
      setActionNotice('Card suspended.')
      if (nextView.current === null) {
        setSessionStats((prev) => ({ ...prev, elapsedMs: Date.now() - sessionStartRef.current }))
      }
    } catch (err: unknown) {
      if (opGenRef.current !== gen || sessionIdRef.current !== sessionId) return
      console.error('study suspend failed', err)
      setActionNotice('Could not suspend card.')
    } finally {
      if (opGenRef.current === gen) inFlightRef.current = false
    }
  }, [storage, view?.current, isPreview])

  const editCurrent = useCallback(() => {
    const card = view?.current
    if (!card) return
    navigate(`/decks/${card.deckId}/cards/${card.id}/edit`, {
      state: { returnTo: `${location.pathname}${location.search}` },
    })
  }, [view?.current, navigate, location.pathname, location.search])

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.isComposing || e.defaultPrevented) return
      const target = e.target as HTMLElement | null
      const tag = target?.tagName ?? ''
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target?.isContentEditable) {
        return
      }
      const noModifiers = !e.metaKey && !e.ctrlKey && !e.altKey
      const isUndoShortcut =
        ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'z')
        || (noModifiers && e.key.toLowerCase() === 'u')
      if (isUndoShortcut && view?.canUndo) {
        e.preventDefault()
        void undoLastGrade()
        return
      }
      if (!view?.current) return
      if (noModifiers && (e.key === 'e' || e.key === 'E')) {
        e.preventDefault()
        editCurrent()
        return
      }
      if (noModifiers && !isPreview && (e.key === 's' || e.key === 'S')) {
        e.preventDefault()
        void suspendCurrent()
        return
      }
      const onInteractiveControl = Boolean(
        target?.closest('button, a, summary, [role="button"], [role="option"]'),
      )
      if (!view.revealed && !schedError) {
        if (noModifiers && !onInteractiveControl && (e.code === 'Space' || e.key === 'Enter')) {
          e.preventDefault()
          reveal()
        }
        return
      }
      if (isPreview && noModifiers && !onInteractiveControl && (e.code === 'Space' || e.key === 'Enter')) {
        e.preventDefault()
        void advancePreview()
        return
      }
      const byKey: Record<string, Grade> = { '1': 'again', '2': 'hard', '3': 'good', '4': 'easy' }
      const keyGrade = noModifiers ? byKey[e.key] : undefined
      if (keyGrade) {
        e.preventDefault()
        void grade(keyGrade)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [view, isPreview, schedError, reveal, advancePreview, grade, editCurrent, undoLastGrade, suspendCurrent])

  const currentCardId = view?.current?.id ?? null
  const isCardRevealed = Boolean(view?.revealed || schedError)
  const showCardTimer = Boolean(view?.showTimer && currentCardId)

  useEffect(() => {
    setCardElapsedSec(0)
  }, [currentCardId, timerAttempt])

  useEffect(() => {
    if (!showCardTimer || isCardRevealed) return
    let accumulatedMs = 0
    let resumeAt: number | null = document.visibilityState === 'hidden' ? null : Date.now()
    const syncElapsed = () => {
      const liveMs = resumeAt !== null ? Date.now() - resumeAt : 0
      setCardElapsedSec(Math.floor((accumulatedMs + liveMs) / 1000))
    }
    const onVisibility = () => {
      if (document.visibilityState === 'hidden') {
        if (resumeAt !== null) {
          accumulatedMs += Date.now() - resumeAt
          resumeAt = null
        }
      } else if (resumeAt === null) {
        resumeAt = Date.now()
      }
      syncElapsed()
    }
    document.addEventListener('visibilitychange', onVisibility)
    const timer = window.setInterval(syncElapsed, 250)
    return () => {
      document.removeEventListener('visibilitychange', onVisibility)
      window.clearInterval(timer)
    }
  }, [showCardTimer, isCardRevealed, currentCardId, timerAttempt])

  if (!ready) return null

  const contextLabel = customTitle ? `${deckName} · ${customTitle}` : deckName
  const terminalTitle = (
    <>
      <span className="header-title-text">{isPreview ? 'Preview' : 'Review'}</span>
      <span className="review-deck">{contextLabel}</span>
    </>
  )
  const terminalAction = <Link to={backTo} className="btn btn-ghost">Close</Link>

  if (startError || !view) {
    return (
      <>
        <PageHeader title={terminalTitle} actions={terminalAction} />
        <div className="review-terminal">
          <div className="empty-state">
            <EmptyIcon kind="skip" />
            <h3>Couldn&#39;t start session</h3>
            <p role="alert">{startError ?? 'Could not start study session.'}</p>
            <Link to={backTo} className="btn btn-ghost cta">{backLabel}</Link>
          </div>
        </div>
      </>
    )
  }

  const current = view.current
  const leechMessage = view.notice === 'suspend'
    ? 'Leech suspended. Edit the card to restore it.'
    : view.notice === 'tag'
      ? 'Leech tagged.'
      : null
  const statusMessage = actionNotice ?? leechMessage

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
              {statusMessage && <p className="review-complete-note" role="status">{statusMessage}</p>}
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
            {statusMessage && <p className="review-complete-note" role="status">{statusMessage}</p>}
            {conflictNotice && <p className="review-complete-note" role="alert">A changed card was skipped instead of grading stale content.</p>}
            <div className="review-complete-actions">
              {!isPreview && view.canUndo && (
                <button
                  type="button"
                  className="btn btn-ghost cta"
                  title="Undo last grade (⌘Z)"
                  onClick={() => void undoLastGrade()}
                >
                  Undo last grade
                </button>
              )}
              <Link to={backTo} className="btn btn-primary cta">{backLabel}</Link>
            </div>
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
            {!isPreview && (
              <>
                <button
                  type="button"
                  className="btn btn-ghost"
                  title="Undo last grade (⌘Z or U)"
                  disabled={!view.canUndo}
                  onClick={() => void undoLastGrade()}
                >
                  Undo
                </button>
                <button
                  type="button"
                  className="btn btn-ghost"
                  title="Suspend card (S)"
                  onClick={() => void suspendCurrent()}
                >
                  Suspend
                </button>
              </>
            )}
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
            {statusMessage && <p className="review-notice" role="status">{statusMessage}</p>}
            {conflictNotice && (
              <p className="review-notice" role="alert">
                A changed card was skipped instead of grading stale content.
              </p>
            )}
            {!displayedRevealed ? (
              <div key={current.id} className="review-card">
                <section className="review-question" aria-label="Question">
                  <div className="review-label-row">
                    <p className="review-label">
                      {current.clozeIndex
                        ? hasImageOcclusion(current.front)
                          ? `Image Occlusion · #${current.clozeIndex}`
                          : `Cloze · c${current.clozeIndex}`
                        : 'Question'}
                    </p>
                    {view.showTimer && (
                      <span className="review-timer" aria-label="Answer timer">
                        {formatTimer(cardElapsedSec)}
                      </span>
                    )}
                  </div>
                  <div className="review-q">
                    <MarkdownView
                      source={current.front}
                      clozeIndex={current.clozeIndex}
                      clozeRevealed={false}
                    />
                  </div>
                </section>
              </div>
            ) : (
              <div key={current.id} className="review-card revealed">
                <section className="review-question" aria-label="Question">
                  <div className="review-label-row">
                    <p className="review-label">
                      {current.clozeIndex
                        ? hasImageOcclusion(current.front)
                          ? `Image Occlusion · #${current.clozeIndex}`
                          : `Cloze · c${current.clozeIndex}`
                        : 'Question'}
                    </p>
                    {view.showTimer && (
                      <span className="review-timer is-paused" aria-label="Answer timer">
                        {formatTimer(cardElapsedSec)}
                      </span>
                    )}
                  </div>
                  <div className="review-q">
                    <MarkdownView
                      source={current.front}
                      clozeIndex={current.clozeIndex}
                      clozeRevealed={true}
                    />
                  </div>
                </section>
                {(!current.clozeIndex || current.back.trim().length > 0) && (
                  <>
                    <hr className="review-rule" />
                    <section className="review-answer" aria-label={current.clozeIndex ? 'Extra' : 'Answer'}>
                      <p className="review-label">{current.clozeIndex ? 'Extra' : 'Answer'}</p>
                      <div className="review-a">
                        <MarkdownView source={current.back} />
                      </div>
                    </section>
                  </>
                )}
                {(current.rationale || (current.sources && current.sources.length > 0) || current.proposedBy) && (
                  <details key={current.id} className="review-provenance">
                    <summary>Source &amp; rationale</summary>
                    <div className="review-provenance-body">
                      {current.rationale && (
                        <p className="review-provenance-rationale">{current.rationale}</p>
                      )}
                      {current.sources && current.sources.length > 0 && (
                        <ul className="review-provenance-sources">
                          {current.sources.map((src, index) => (
                            <li key={`${src.locator}-${index}`}>
                              {src.label ? <strong>{src.label}: </strong> : null}
                              <code>{src.locator}</code>
                            </li>
                          ))}
                        </ul>
                      )}
                      {current.proposedBy && (
                        <p className="review-provenance-meta">Proposed by {current.proposedBy}</p>
                      )}
                    </div>
                  </details>
                )}
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
