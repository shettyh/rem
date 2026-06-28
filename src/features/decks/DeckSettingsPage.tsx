import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { Storage } from '../../data/Storage'
import type { Deck, DeckSettings } from '../../domain/models'
import { PageHeader } from '../../ui/PageHeader'
import { Stepper } from '../../ui/Stepper'
import { DECK_PALETTE, deckColor } from '../../ui/deckColor'

/** Split a space-separated steps string into chip tokens. */
export function parseSteps(raw: string): string[] {
  return raw.split(/\s+/).filter(Boolean)
}

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

  const title = (
    <>
      <button className="back-link" aria-label="Back to deck" onClick={() => navigate(`/decks/${deck.id}`)}>
        ‹ {deck.name}
      </button>
      <span className="header-dot" style={{ background: color || deckColor(deck.id) }} />
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
              onBlur={() => storage.updateDeck(deck.id, { name })}
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
          </div>
        </div>
      </div>
    </>
  )
}
