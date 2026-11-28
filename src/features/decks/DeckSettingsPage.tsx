import { useEffect, useRef, useState } from 'react'
import { useLocation, useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { useStorageQuery } from '../../data/useStorageQuery'
import type { Storage } from '../../data/Storage'
import type { Deck, DeckSettings } from '../../domain/models'
import { PageHeader } from '../../ui/PageHeader'
import { Stepper } from '../../ui/Stepper'
import { SegToggle } from '../../ui/SegToggle'
import { parseSteps, parseStepsMs } from '../../domain/scheduler/steps'
import {
  buildReviewHistories,
  getFsrsOptimizer,
  hasDelayedReview,
} from '../../domain/scheduler/optimizer'
import {
  CUSTOM_STUDY_PRESETS,
  customStudyPreset,
  type CustomStudyMode,
} from '../review/customStudy'
import {
  deckIsStrictDescendant,
  loadCollapsedDeckPaths,
  pruneCollapsedDeckPaths,
  saveCollapsedDeckPaths,
} from './deckTree'

export function DeckSettingsPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const deck = useStorageQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  if (!deckId || deck === undefined) return null
  // Remount on deck change so local form state re-seeds from storage.
  return <DeckSettingsForm key={deck.id} deck={deck} storage={storage} />
}

function DeckSettingsForm({ deck, storage }: { deck: Deck; storage: Storage }) {
  const navigate = useNavigate()
  const location = useLocation()
  const [name, setName] = useState(deck.name)
  const [nameError, setNameError] = useState<string | null>(null)
  const [settings, setSettings] = useState<DeckSettings>(deck.settings)
  const settingsRef = useRef<DeckSettings>(deck.settings)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [customMode, setCustomMode] = useState<CustomStudyMode | null>(null)
  const [customAmount, setCustomAmount] = useState(1)
  const [optimizing, setOptimizing] = useState(false)
  const [optimizerError, setOptimizerError] = useState(false)
  const reviewLogs = useStorageQuery(() => storage.listReviewLogs(deck.id), [deck.id]) ?? []
  const subtreeInfo = useStorageQuery(async () => {
    const allDecks = await storage.listDecks()
    const descendants = allDecks.filter(
      (candidate) => candidate.id !== deck.id && deckIsStrictDescendant(candidate.name, deck.name),
    )
    const directCards = await storage.listCards(deck.id)
    const childCards = await Promise.all(descendants.map((child) => storage.listCards(child.id)))
    const cardCount = directCards.length + childCards.reduce((sum, list) => sum + list.length, 0)
    return { subdeckCount: descendants.length, cardCount }
  }, [deck.id, deck.name])
  const subdeckCount = subtreeInfo?.subdeckCount ?? 0
  const subtreeCardCount = subtreeInfo?.cardCount ?? 0
  const reviewHistories = buildReviewHistories(reviewLogs)
  const canOptimize = hasDelayedReview(reviewHistories)

  useEffect(() => {
    if (location.hash === '#custom-study') {
      document.getElementById('custom-study')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    }
  }, [location.hash])

  const customPreset = customMode ? customStudyPreset(customMode) : null
  function selectCustomMode(mode: CustomStudyMode) {
    const preset = customStudyPreset(mode)
    setCustomMode(mode)
    setCustomAmount(preset.defaultAmount)
  }

  async function persistSettings(next: DeckSettings) {
    try {
      await storage.updateDeck(deck.id, { settings: next })
      setSaveError(null)
    } catch (err) {
      console.error('deck settings update failed', err)
      setSaveError(err instanceof Error ? err.message : 'Could not save deck options.')
    }
  }

  /** Update one setting and persist immediately (steppers, toggles, segmented).
   *  The `as DeckSettings` cast is required: TS can't narrow a generic
   *  computed-key spread under strict mode. */
  function set<K extends keyof DeckSettings>(key: K, value: DeckSettings[K]) {
    const next = { ...settingsRef.current, [key]: value } as DeckSettings
    settingsRef.current = next
    setSettings(next)
    void persistSettings(next)
  }
  /** Update one setting locally only (text inputs persist on blur via commit). */
  function setLocal<K extends keyof DeckSettings>(key: K, value: DeckSettings[K]) {
    const next = { ...settingsRef.current, [key]: value } as DeckSettings
    settingsRef.current = next
    setSettings(next)
  }
  function commit() {
    void persistSettings(settingsRef.current)
  }
  async function optimizeParameters() {
    if (!canOptimize || optimizing) return
    setOptimizing(true)
    setOptimizerError(false)
    try {
      const weights = await getFsrsOptimizer().optimize(
        reviewHistories,
        parseStepsMs(settingsRef.current.relearnSteps).length,
      )
      const next: DeckSettings = { ...settingsRef.current, fsrsWeights: weights }
      settingsRef.current = next
      setSettings(next)
      await persistSettings(next)
    } catch (error) {
      console.error('FSRS optimization failed', error)
      setOptimizerError(true)
    } finally {
      setOptimizing(false)
    }
  }
  async function resetParameters() {
    const next: DeckSettings = { ...settingsRef.current, fsrsWeights: null }
    settingsRef.current = next
    setSettings(next)
    setOptimizerError(false)
    await persistSettings(next)
  }

  const title = (
    <>
      <button className="back-link" aria-label="Back to deck" onClick={() => navigate(`/decks/${deck.id}`)}>
        ‹ {deck.name}
      </button>
      <span className="header-title-text">Deck options</span>
    </>
  )

  return (
    <>
      <PageHeader title={title} />
      <div className="page-body">
        <div className="deck-settings">
          {saveError && <div className="ds-row-sub ds-error" role="alert">{saveError}</div>}
          {/* GENERAL */}
          <div className="ds-label">General</div>
          <div className="ds-card">
            <label className="ds-field-label" htmlFor="ds-name">Deck name</label>
            <input
              id="ds-name"
              className="ds-name-input"
              value={name}
              onChange={(e) => {
                setName(e.target.value)
                setNameError(null)
              }}
              onBlur={async () => {
                try {
                  await storage.updateDeck(deck.id, { name })
                  setNameError(null)
                } catch (err) {
                  setNameError(err instanceof Error ? err.message : 'Invalid deck name.')
                }
              }}
            />
            {nameError && <div className="ds-row-sub ds-error" role="alert">{nameError}</div>}
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Desired retention</div>
                <div className="ds-row-sub">Target recall probability. Higher values schedule more frequent reviews.</div>
              </div>
              <Stepper
                value={settings.desiredRetention}
                onChange={(v) => set('desiredRetention', Math.round(v * 100) / 100)}
                label="Desired retention"
                step={0.01}
                min={0.7}
                max={0.99}
                format={(v) => `${Math.round(v * 100)}%`}
              />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Personalized scheduling</div>
                <div className="ds-row-sub">
                  {settings.fsrsWeights ? 'Personalized schedule' : 'Default schedule'}
                </div>
                <div className="ds-row-sub">
                  {reviewLogs.length} recorded review{reviewLogs.length === 1 ? '' : 's'}
                </div>
                {!canOptimize && (
                  <div className="ds-row-sub">Optimize after reviewing a card on a later day.</div>
                )}
                {optimizerError && <div className="ds-row-sub ds-error" role="alert">Couldn&#39;t optimize schedule. Try again.</div>}
              </div>
              <div className="row">
                {settings.fsrsWeights && (
                  <button type="button" className="btn btn-ghost" disabled={optimizing} onClick={() => void resetParameters()}>
                    Reset
                  </button>
                )}
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={!canOptimize || optimizing}
                  onClick={() => void optimizeParameters()}
                >
                  {optimizing ? 'Optimizing…' : 'Optimize'}
                </button>
              </div>
            </div>
          </div>

          {/* DAILY LIMITS */}
          <div className="ds-label">Daily limits</div>
          <div className="ds-card">
            <div className="ds-row">
              <div>
                <div className="ds-row-title">New cards/day</div>
                <div className="ds-row-sub">Cap on new cards introduced daily.</div>
              </div>
              <Stepper value={settings.newPerDay} onChange={(v) => set('newPerDay', v)} label="New cards/day" step={5} min={0} max={9999} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Maximum reviews/day</div>
                <div className="ds-row-sub">Cap on due reviews shown each day.</div>
              </div>
              <Stepper value={settings.maxReviews} onChange={(v) => set('maxReviews', v)} label="Maximum reviews/day" step={10} min={0} max={9999} />
            </div>
          </div>

          {/* NEW CARDS */}
          <div className="ds-label">New cards</div>
          <div className="ds-card">
            <div className="ds-row">
              <div className="ds-row-title">Learning steps</div>
              <span className="ds-row-sub" style={{ fontFamily: 'var(--font-mono)' }}>e.g. 1m 10m</span>
            </div>
            <div className="ds-row-sub" style={{ margin: '3px 0 12px' }}>Intervals a new card steps through before graduating. Space-separated.</div>
            <input
              className="ds-steps-input"
              aria-label="Learning steps"
              value={settings.learnSteps}
              onChange={(e) => setLocal('learnSteps', e.target.value)}
              onBlur={commit}
            />
            <div className="ds-chips">
              {parseSteps(settings.learnSteps).map((s, i) => (
                <span key={`${s}-${i}`} className="ds-chip">{s}</span>
              ))}
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Insertion order</div>
                <div className="ds-row-sub">Order new cards enter the queue.</div>
              </div>
              <SegToggle
                value={settings.insertionOrder}
                onChange={(v) => set('insertionOrder', v)}
                options={[{ value: 'sequential', label: 'SEQ' }, { value: 'random', label: 'RANDOM' }]}
              />
            </div>
          </div>

          {/* LAPSES */}
          <div className="ds-label">Lapses</div>
          <div className="ds-card">
            <div className="ds-row">
              <div className="ds-row-title">Relearn steps</div>
              <span className="ds-row-sub" style={{ fontFamily: 'var(--font-mono)' }}>e.g. 10m</span>
            </div>
            <div className="ds-row-sub" style={{ margin: '3px 0 12px' }}>Steps a lapsed card relearns through. Space-separated.</div>
            <input
              className="ds-steps-input"
              aria-label="Relearn steps"
              value={settings.relearnSteps}
              onChange={(e) => setLocal('relearnSteps', e.target.value)}
              onBlur={commit}
            />
            <div className="ds-chips">
              {parseSteps(settings.relearnSteps).map((s, i) => (
                <span key={`${s}-${i}`} className="ds-chip is-lapse">{s}</span>
              ))}
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Minimum interval</div>
                <div className="ds-row-sub">Floor for intervals after a lapse.</div>
              </div>
              <Stepper value={settings.minimumInterval} onChange={(v) => set('minimumInterval', v)} label="Minimum interval" step={1} min={1} max={365} format={(v) => `${v}d`} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Leech threshold</div>
                <div className="ds-row-sub">Lapses before a card is flagged a leech.</div>
              </div>
              <Stepper value={settings.leechThreshold} onChange={(v) => set('leechThreshold', v)} label="Leech threshold" step={1} min={1} max={99} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Leech action</div>
                <div className="ds-row-sub">What happens when a leech is found.</div>
              </div>
              <SegToggle
                value={settings.leechAction}
                onChange={(v) => set('leechAction', v)}
                options={[{ value: 'tag', label: 'TAG' }, { value: 'suspend', label: 'SUSPEND' }]}
              />
            </div>
          </div>

          {/* STUDY BEHAVIOR */}
          <div className="ds-label">Study</div>
          <div className="ds-card">
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Bury related siblings</div>
                <div className="ds-row-sub">Defer sibling cloze and reversed cards until tomorrow after studying one.</div>
              </div>
              <SegToggle
                value={settings.buryRelated ? 'on' : 'off'}
                onChange={(v) => set('buryRelated', v === 'on')}
                options={[{ value: 'off', label: 'OFF' }, { value: 'on', label: 'ON' }]}
              />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Show answer timer</div>
                <div className="ds-row-sub">Display a quiet elapsed timer while recalling each card.</div>
              </div>
              <SegToggle
                value={settings.showTimer ? 'on' : 'off'}
                onChange={(v) => set('showTimer', v === 'on')}
                options={[{ value: 'off', label: 'OFF' }, { value: 'on', label: 'ON' }]}
              />
            </div>
          </div>

          {/* CUSTOM STUDY */}
          <div id="custom-study" className="ds-label">Custom study</div>
          <div className="ds-grid">
            {CUSTOM_STUDY_PRESETS.map((preset) => (
              <button
                key={preset.mode}
                type="button"
                className={preset.mode === customMode ? 'ds-preset is-active' : 'ds-preset'}
                aria-pressed={preset.mode === customMode}
                onClick={() => selectCustomMode(preset.mode)}
              >
                <span className="ds-preset-title">{preset.title}</span>
                <span className="ds-preset-sub">{preset.description}</span>
              </button>
            ))}
          </div>
          <div className="ds-custom-run">
            <div className="ds-row-title">{customPreset?.title ?? 'Select a preset'}</div>
            {customPreset && (
              <Stepper
                value={customAmount}
                onChange={setCustomAmount}
                label={`${customPreset.title} ${customPreset.unit}`}
                step={customPreset.step}
                min={1}
                max={999}
                format={(value) => `${value} ${customPreset.unit === 'days' ? `day${value === 1 ? '' : 's'}` : `card${value === 1 ? '' : 's'}`}`}
              />
            )}
            <button
              type="button"
              className="btn btn-primary"
              disabled={!customMode}
              onClick={() => navigate(`/decks/${deck.id}/study?custom=${customMode}&amount=${customAmount}`)}
            >
              Start
            </button>
          </div>

          {/* DANGER ZONE */}
          <div className="ds-label is-danger">Danger zone</div>
          <div className="ds-card is-danger">
            <div className="ds-row">
              <div>
                <div className="ds-row-title">
                  {subdeckCount > 0 ? 'Delete this deck and subdecks' : 'Delete this deck'}
                </div>
                <div className="ds-row-sub">
                  {subdeckCount > 0
                    ? `Permanently removes this deck, ${subdeckCount} ${subdeckCount === 1 ? 'subdeck' : 'subdecks'}, and ${subtreeCardCount} ${subtreeCardCount === 1 ? 'card' : 'cards'} total. This can't be undone.`
                    : "Permanently removes the deck and all its cards. This can't be undone."}
                </div>
              </div>
              {confirmDelete ? (
                <div className="row">
                  <button
                    type="button"
                    className="btn btn-danger"
                    onClick={async () => {
                      try {
                        await storage.deleteDeck(deck.id)
                        await storage.sweepOrphanAssets()
                        saveCollapsedDeckPaths(
                          pruneCollapsedDeckPaths(loadCollapsedDeckPaths(), [deck.name]),
                        )
                        navigate('/')
                      } catch (err) {
                        console.error('deck delete failed', err)
                        setSaveError(err instanceof Error ? err.message : 'Could not delete deck.')
                      }
                    }}
                  >
                    Confirm delete
                  </button>
                  <button type="button" className="btn btn-ghost" onClick={() => setConfirmDelete(false)}>
                    Cancel
                  </button>
                </div>
              ) : (
                <button type="button" className="btn btn-danger-outline" onClick={() => setConfirmDelete(true)}>
                  {subdeckCount > 0 ? 'Delete deck & subdecks' : 'Delete deck'}
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  )
}
