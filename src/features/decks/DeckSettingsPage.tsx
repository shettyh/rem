import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { Storage } from '../../data/Storage'
import type { Deck, DeckSettings } from '../../domain/models'
import { PageHeader } from '../../ui/PageHeader'
import { Stepper } from '../../ui/Stepper'
import { SegToggle } from '../../ui/SegToggle'
import { DECK_PALETTE, deckColor } from '../../ui/deckColor'
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

export function DeckSettingsPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  if (!deckId || deck === undefined) return null
  // Remount on deck change so local form state re-seeds from storage.
  return <DeckSettingsForm key={deck.id} deck={deck} storage={storage} />
}

function DeckSettingsForm({ deck, storage }: { deck: Deck; storage: Storage }) {
  const navigate = useNavigate()
  const [name, setName] = useState(deck.name)
  const [color, setColor] = useState(deck.color)
  const [settings, setSettings] = useState<DeckSettings>(deck.settings)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [customMode, setCustomMode] = useState<CustomStudyMode | null>(null)
  const [customAmount, setCustomAmount] = useState(1)
  const [optimizing, setOptimizing] = useState(false)
  const [optimizerError, setOptimizerError] = useState(false)
  const reviewLogs = useLiveQuery(() => storage.listReviewLogs(deck.id), [deck.id, storage]) ?? []
  const reviewHistories = buildReviewHistories(reviewLogs)
  const canOptimize = hasDelayedReview(reviewHistories)

  const customPreset = customMode ? customStudyPreset(customMode) : null
  function selectCustomMode(mode: CustomStudyMode) {
    const preset = customStudyPreset(mode)
    setCustomMode(mode)
    setCustomAmount(preset.defaultAmount)
  }

  function pickColor(c: string) {
    setColor(c)
    void storage.updateDeck(deck.id, { color: c })
  }
  /** Update one setting and persist immediately (steppers, toggles, segmented).
   *  The `as DeckSettings` cast is required: TS can't narrow a generic
   *  computed-key spread under strict mode. */
  function set<K extends keyof DeckSettings>(key: K, value: DeckSettings[K]) {
    const next = { ...settings, [key]: value } as DeckSettings
    setSettings(next)
    void storage.updateDeck(deck.id, { settings: next })
  }
  /** Update one setting locally only (text inputs persist on blur via commit). */
  function setLocal<K extends keyof DeckSettings>(key: K, value: DeckSettings[K]) {
    setSettings((s) => ({ ...s, [key]: value }) as DeckSettings)
  }
  function commit() {
    void storage.updateDeck(deck.id, { settings })
  }
  async function optimizeParameters() {
    if (!canOptimize || optimizing) return
    setOptimizing(true)
    setOptimizerError(false)
    try {
      const weights = await getFsrsOptimizer().optimize(
        reviewHistories,
        parseStepsMs(settings.relearnSteps).length,
      )
      const next = { ...settings, fsrsWeights: weights }
      setSettings(next)
      await storage.updateDeck(deck.id, { settings: next })
    } catch (error) {
      console.error('FSRS optimization failed', error)
      setOptimizerError(true)
    } finally {
      setOptimizing(false)
    }
  }
  async function resetParameters() {
    const next = { ...settings, fsrsWeights: null }
    setSettings(next)
    setOptimizerError(false)
    await storage.updateDeck(deck.id, { settings: next })
  }

  const title = (
    <>
      <button className="back-link" aria-label="Back to deck" onClick={() => navigate(`/decks/${deck.id}`)}>
        ‹ {deck.name}
      </button>
      <span className="header-dot" style={{ background: color ?? deckColor(deck.id) }} />
      <span className="header-title-text">Deck options</span>
    </>
  )

  return (
    <>
      <PageHeader title={title} />
      <div className="page-body">
        <div className="deck-settings">
          {/* GENERAL */}
          <div className="ds-label">General</div>
          <div className="ds-card">
            <label className="ds-field-label" htmlFor="ds-name">Deck name</label>
            <input
              id="ds-name"
              className="ds-name-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onBlur={() => void storage.updateDeck(deck.id, { name })}
            />
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Color</div>
                <div className="ds-row-sub">Shown in the sidebar and on cards.</div>
              </div>
              <div className="ds-swatches">
                {DECK_PALETTE.map((c) => (
                  <button
                    key={c}
                    type="button"
                    aria-label={`Color ${c}`}
                    aria-pressed={c === color}
                    className={c === color ? 'ds-swatch is-active' : 'ds-swatch'}
                    style={{ background: c }}
                    onClick={() => pickColor(c)}
                  />
                ))}
              </div>
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Scheduler</div>
                <div className="ds-row-sub">FSRS adapts intervals to your recall.</div>
              </div>
              <span className="algo-chip">FSRS</span>
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Desired retention</div>
                <div className="ds-row-sub">Target probability of recall at review time.</div>
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
                <div className="ds-row-title">FSRS parameters</div>
                <div className="ds-row-sub">
                  {settings.fsrsWeights ? 'Optimized parameters' : 'Default parameters'}
                </div>
                <div className="ds-row-sub">
                  {reviewLogs.length} recorded FSRS review{reviewLogs.length === 1 ? '' : 's'}
                </div>
                {!canOptimize && (
                  <div className="ds-row-sub">Optimize after reviewing a card on a later day.</div>
                )}
                {optimizerError && <div className="ds-row-sub ds-error" role="alert">Couldn&#39;t optimize parameters. Try again.</div>}
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
              <span className="ds-row-sub" style={{ fontFamily: 'var(--font-mono)' }}>e.g. 1m 10m 1d</span>
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

          {/* CUSTOM STUDY */}
          <div className="ds-label">Custom study</div>
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
                <div className="ds-row-title">Delete this deck</div>
                <div className="ds-row-sub">Permanently removes the deck and all its cards. This can't be undone.</div>
              </div>
              {confirmDelete ? (
                <div className="row">
                  <button
                    type="button"
                    className="btn btn-danger"
                    onClick={async () => {
                      await storage.deleteDeck(deck.id)
                      navigate('/')
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
                  Delete deck
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  )
}
