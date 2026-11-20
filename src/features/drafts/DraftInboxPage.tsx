import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import type { CardDraft, DraftResolution } from '../../domain/models'
import { EmptyIcon } from '../../ui/EmptyIcon'
import { PageHeader } from '../../ui/PageHeader'
import { EditorToolbar } from '../cards/EditorToolbar'
import { MarkdownView } from '../cards/MarkdownView'
import { parseUserTags } from '../cards/cardTags'
import { RichMarkdownEditor, type EditorHandle } from '../cards/RichMarkdownEditor'

interface DraftEdit {
  front: string
  back: string
  tags: string
  deckId: string
}

const EMPTY_EDIT: DraftEdit = { front: '', back: '', tags: '', deckId: '' }

/** Oldest-first human approval flow for local agent proposals. */
export function DraftInboxPage() {
  const storage = useStorage()
  const [refresh, setRefresh] = useState(0)
  const data = useStorageQuery(async () => {
    const [drafts, decks] = await Promise.all([storage.listDrafts(), storage.listDecks()])
    return { drafts, decks }
  }, [refresh])
  const [deferredIds, setDeferredIds] = useState<string[]>([])
  const orderedDrafts = useMemo(() => {
    const list = data?.drafts ?? []
    if (deferredIds.length === 0) return list
    const deferred = new Set(deferredIds)
    return [
      ...list.filter((d) => !deferred.has(d.id)),
      ...deferredIds.map((id) => list.find((d) => d.id === id)).filter((d): d is CardDraft => Boolean(d)),
    ]
  }, [data?.drafts, deferredIds])
  const current = orderedDrafts[0]
  const pending = orderedDrafts.length
  const [loadedId, setLoadedId] = useState<string>()
  const [edit, setEdit] = useState<DraftEdit>(EMPTY_EDIT)
  const [revealed, setRevealed] = useState(false)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<string>()
  const [error, setError] = useState<string>()
  const [active, setActive] = useState<EditorHandle | null>(null)
  const frontHandle = useRef<EditorHandle | null>(null)
  const backHandle = useRef<EditorHandle | null>(null)

  useEffect(() => {
    if (!current || current.id === loadedId) return
    setLoadedId(current.id)
    setEdit({
      front: current.front,
      back: current.back,
      tags: current.tags.join(', '),
      deckId: current.deckId,
    })
    setRevealed(false)
    setError(undefined)
    setActive(null)
  }, [current, loadedId])

  const onFrontReady = useCallback((handle: EditorHandle) => {
    frontHandle.current = handle
    setActive((value) => value ?? handle)
  }, [])
  const onBackReady = useCallback((handle: EditorHandle) => {
    backHandle.current = handle
  }, [])

  const resolve = useCallback(async (draft: CardDraft, action: 'accept' | 'reject') => {
    setBusy(true)
    setError(undefined)
    try {
      const resolution = await storage.resolveDraft(
        draft.id,
        draft.revision,
        action === 'reject'
          ? { decision: 'reject' }
          : {
              decision: 'accept',
              deckId: edit.deckId,
              card: {
                front: edit.front,
                back: edit.back,
                tags: parseUserTags(edit.tags),
              },
            },
      )
      setDeferredIds((ids) => ids.filter((id) => id !== draft.id))
      setMessage(resolutionMessage(resolution))
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not resolve this draft.')
    } finally {
      setBusy(false)
    }
  }, [storage, edit])

  function skipCurrent() {
    if (!current || pending <= 1) return
    setDeferredIds((ids) => [...ids.filter((id) => id !== current.id), current.id])
    setMessage(undefined)
  }

  // Space/Enter reveals an unrevealed proposal; ⌘⏎/Ctrl+⏎ accepts a revealed one.
  useEffect(() => {
    if (!current || current.id !== loadedId) return
    function onKey(e: KeyboardEvent) {
      if (!revealed) {
        const tag = (e.target as HTMLElement | null)?.tagName ?? ''
        if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || tag === 'BUTTON' || tag === 'A') return
        if (e.code === 'Space' || e.key === 'Enter') {
          e.preventDefault()
          setRevealed(true)
        }
        return
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && !busy && edit.front.trim() && edit.deckId) {
        e.preventDefault()
        void resolve(current, 'accept')
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [current, loadedId, revealed, busy, edit.front, edit.deckId, resolve])

  const title = (
    <>
      <span className="header-title-text">Drafts</span>
      {pending > 0 && <span className="header-deck-chip">{pending} pending</span>}
    </>
  )

  if (!data) return <PageHeader title={title} />

  if (!current) {
    return (
      <>
        <PageHeader title={title} />
        <div className="draft-terminal">
          <div className="empty-state">
            <EmptyIcon kind="clear" />
            <h3>Inbox clear</h3>
            <p>{message ?? 'Agent proposals will wait here for your approval.'}</p>
          </div>
        </div>
      </>
    )
  }

  // Do not briefly render the next proposal with the previous draft's edit state.
  if (current.id !== loadedId) return <PageHeader title={title} />

  return (
    <>
      <PageHeader title={title} />
      <div className="draft-inbox">
        {message && <p className="draft-notice" role="status">{message}</p>}
        {error && (
          <div className="draft-error" role="alert">
            <span>{error}</span>
            <button
              type="button"
              onClick={() => {
                setError(undefined)
                setRefresh((value) => value + 1)
              }}
            >
              Reload inbox
            </button>
          </div>
        )}

        <section className="draft-triage" aria-label={`Draft ${current.id}`}>
          <div className="draft-sequence">
            <span>{pending} pending</span>
            {current.proposedBy && <span>Proposed by {current.proposedBy}</span>}
          </div>

          {!revealed ? (
            <div className="draft-prompt">
              <div className="draft-prompt-inner">
                <p className="draft-eyebrow">Try answering before you reveal</p>
                <div className="draft-question">
                  <MarkdownView source={current.front} />
                </div>
                <div className="draft-prompt-actions">
                  <button
                    type="button"
                    className="btn btn-primary"
                    onClick={() => setRevealed(true)}
                  >
                    Reveal proposal <span className="kbd" aria-hidden="true">space</span>
                  </button>
                  {pending > 1 && (
                    <button type="button" className="btn btn-ghost" onClick={skipCurrent}>
                      Skip for now
                    </button>
                  )}
                </div>
              </div>
            </div>
          ) : (
            <div className="draft-edit">
              <div className="md-toolbar-bar">
                <EditorToolbar editor={active?.editor ?? null} />
              </div>
              <section className="card-editor-surface" aria-label="Draft content">
                <div className="editor-field editor-field--front">
                  <div className="editor-field-label">Front</div>
                  <RichMarkdownEditor
                    value={edit.front}
                    onChange={(front) => setEdit((value) => ({ ...value, front }))}
                    placeholder="Type the prompt…"
                    ariaLabel="Draft front"
                    onReady={onFrontReady}
                    onFocus={() => frontHandle.current && setActive(frontHandle.current)}
                  />
                </div>
                <div className="editor-field editor-field--back">
                  <div className="editor-field-label">Proposed back</div>
                  <RichMarkdownEditor
                    value={edit.back}
                    onChange={(back) => setEdit((value) => ({ ...value, back }))}
                    placeholder="Type the answer…"
                    ariaLabel="Draft back"
                    onReady={onBackReady}
                    onFocus={() => backHandle.current && setActive(backHandle.current)}
                  />
                </div>
              </section>

              <div className="draft-fields">
                <label>
                  <span>Target deck</span>
                  <select
                    aria-label="Target deck"
                    value={edit.deckId}
                    onChange={(event) => setEdit((value) => ({
                      ...value,
                      deckId: event.target.value,
                    }))}
                  >
                    {data.decks.map((deck) => (
                      <option key={deck.id} value={deck.id}>{deck.name}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>Tags</span>
                  <input
                    className="text-input"
                    aria-label="Tags"
                    value={edit.tags}
                    onChange={(event) => setEdit((value) => ({
                      ...value,
                      tags: event.target.value,
                    }))}
                  />
                </label>
              </div>

              {(current.rationale || current.sources.length > 0) && (
                <aside className="draft-provenance" aria-label="Proposal context">
                  {current.rationale && (
                    <div>
                      <h2>Why remember this</h2>
                      <p>{current.rationale}</p>
                    </div>
                  )}
                  {current.sources.length > 0 && (
                    <div>
                      <h2>Sources</h2>
                      <ul>
                        {current.sources.map((source, index) => (
                          <li key={`${source.locator}:${index}`}>
                            {source.label && <span>{source.label}</span>}
                            <code>{source.locator}</code>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </aside>
              )}

              <div className="draft-actions">
                <div className="draft-actions-left">
                  <button
                    type="button"
                    className="btn btn-danger-outline"
                    disabled={busy}
                    onClick={() => void resolve(current, 'reject')}
                  >
                    Reject draft
                  </button>
                  {pending > 1 && (
                    <button type="button" className="btn btn-ghost" disabled={busy} onClick={skipCurrent}>
                      Skip
                    </button>
                  )}
                </div>
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={busy || !edit.front.trim() || !edit.deckId}
                  onClick={() => void resolve(current, 'accept')}
                >
                  Accept draft
                </button>
              </div>
            </div>
          )}
        </section>
      </div>
    </>
  )
}

function resolutionMessage(resolution: DraftResolution): string {
  switch (resolution.status) {
    case 'accepted':
      return 'Draft accepted.'
    case 'existingCard':
      return 'Card already existed; draft removed.'
    case 'rejected':
      return 'Draft rejected.'
  }
}
