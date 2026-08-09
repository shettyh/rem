import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import type { Grade, StudyRequest, StudyView } from '../../domain/models'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import { PageHeader } from '../../ui/PageHeader'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'
import {
  customStudyPreset,
  parseCustomStudyRequest,
} from './customStudy'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const [searchParams] = useSearchParams()
  const customRequest = deckId ? parseCustomStudyRequest(searchParams) : null
  const customMode = customRequest?.mode ?? null
  const isPreview = customMode === 'preview-new'

  const sessionIdRef = useRef<string | null>(null)
  const gradingRef = useRef(false)
  const [view, setView] = useState<StudyView | null>(null)
  const [ready, setReady] = useState(false)
  const [revealedAt, setRevealedAt] = useState(0)
  const [schedError, setSchedError] = useState(false)
  const [conflictNotice, setConflictNotice] = useState(false)

  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const deckName = deckId ? (deck?.name ?? 'Deck') : 'All decks'
  const customTitle = customMode ? customStudyPreset(customMode).title : null
  const backTo = customMode && deckId ? `/decks/${deckId}/options` : deckId ? `/decks/${deckId}` : '/'
  const backLabel = customMode ? 'Back to options' : deckId ? 'Back to deck' : 'Back to Today'

  useEffect(() => {
    let active = true
    let startedId: string | null = null
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
      setView(await storage.advanceStudyPreview(sessionId))
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
      } finally {
        gradingRef.current = false
      }
    },
    [storage, view],
  )

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!view?.current) return
      if (!view.revealed && !schedError) {
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
      const keyGrade = byKey[e.key]
      if (keyGrade) {
        e.preventDefault()
        void grade(keyGrade)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [view, isPreview, schedError, reveal, advancePreview, grade])

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
              <div className="ico" aria-hidden="true">SKIP</div>
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
                ? `${view.reviewed} card${view.reviewed === 1 ? '' : 's'} previewed.`
                : `${view.reviewed} review${view.reviewed === 1 ? '' : 's'} done. Nice work.`}
            </p>
            {leechMessage && <p role="status">{leechMessage}</p>}
            {conflictNotice && <p role="alert">A changed card was skipped instead of grading stale content.</p>}
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
        {conflictNotice && (
          <p className="review-notice" role="alert">
            A changed card was skipped instead of grading stale content.
          </p>
        )}
        {!displayedRevealed ? (
          <div className="review-stage">
            <div className="review-card">
              <div className="review-q">
                <MarkdownView source={current.front} />
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
                <MarkdownView source={current.front} />
              </div>
              <hr className="review-rule" />
              <p className="answer-label">Answer</p>
              <div className="review-a">
                <MarkdownView source={current.back} />
              </div>
            </div>
            <div className="review-actions">
              {isPreview ? (
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
        )}
      </div>
    </>
  )
}
