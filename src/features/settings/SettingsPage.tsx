import { useRef, useState, type ChangeEvent } from 'react'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { SyncSection } from './SyncSection'
import {
  collectBackup,
  serializeBackup,
  parseBackup,
  planImport,
  type DeckBackup,
} from '../../data/backup'

function downloadJson(json: string, filename: string) {
  const url = URL.createObjectURL(new Blob([json], { type: 'application/json' }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

function todayStamp(): string {
  return new Date().toISOString().slice(0, 10)
}

export function SettingsPage() {
  const storage = useStorage()
  const decks = useLiveQuery(() => storage.listDecks(), [])

  const importInput = useRef<HTMLInputElement>(null)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [showDeckPicker, setShowDeckPicker] = useState(false)
  const [pending, setPending] = useState<{ decks: DeckBackup[]; replaced: string[] } | null>(null)
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const list = decks ?? []
  const allSelected = list.length > 0 && list.every((d) => selected.has(d.id))

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  function toggleAll() {
    setSelected(allSelected ? new Set() : new Set(list.map((d) => d.id)))
  }

  async function exportDecks(ids: string[]) {
    const payload = await collectBackup(storage, ids)
    downloadJson(serializeBackup(payload, Date.now()), `rem-backup-${todayStamp()}.json`)
  }

  async function runImport(toImport: DeckBackup[]) {
    const result = await storage.importDecks(toImport)
    setPending(null)
    const replacedNote = result.replaced.length ? ` (replaced ${result.replaced.length})` : ''
    setMessage(`Imported ${toImport.length} deck${toImport.length === 1 ? '' : 's'}${replacedNote}.`)
  }

  async function onFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = '' // allow re-selecting the same file later
    if (!file) return
    setError(null)
    setMessage(null)
    setPending(null)
    let parsed: DeckBackup[]
    try {
      parsed = parseBackup(await file.text(), Date.now())
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not read the file.')
      return
    }
    const { replaced } = planImport(
      parsed.map((d) => d.name),
      list.map((d) => d.name),
    )
    if (replaced.length > 0) setPending({ decks: parsed, replaced })
    else await runImport(parsed)
  }

  return (
    <>
      <PageHeader title="Settings" />
      <div className="settings-body">
        <section className="settings-group" aria-labelledby="data-storage-heading">
          <div className="settings-group-heading">
            <h2 id="data-storage-heading">Data &amp; storage</h2>
            <p>Your collection lives on this device and remains available without sync.</p>
          </div>

          <div className="settings-panel">
            <div className="settings-panel-summary">
              <div className="settings-panel-copy">
                <div className="settings-title-line">
                  <h3>Local storage</h3>
                  <span className="settings-badge is-active">Active</span>
                </div>
                <p>Decks, review history, and media are stored privately in rem’s app data.</p>
              </div>
            </div>

            <div className="settings-action-row">
              <div className="settings-action-copy">
                <strong>Export backup</strong>
                <span>Save a portable JSON copy of all decks or choose specific decks.</span>
              </div>
              <div className="settings-inline-actions">
                <button
                  className="btn btn-ghost"
                  type="button"
                  disabled={list.length === 0}
                  aria-expanded={showDeckPicker}
                  onClick={() => setShowDeckPicker((shown) => !shown)}
                >
                  Choose decks
                </button>
                <button
                  className="btn"
                  type="button"
                  disabled={list.length === 0}
                  onClick={() => void exportDecks(list.map((d) => d.id))}
                >
                  Export all
                </button>
              </div>
            </div>

            {showDeckPicker && (
              <div className="settings-deck-picker">
                <div className="check-list">
                  <label className="check">
                    <input
                      type="checkbox"
                      aria-label="Select all decks"
                      checked={allSelected}
                      onChange={toggleAll}
                    />
                    Select all
                  </label>
                  {list.map((d) => (
                    <label key={d.id} className="check">
                      <input
                        type="checkbox"
                        checked={selected.has(d.id)}
                        onChange={() => toggle(d.id)}
                      />
                      {d.name}
                    </label>
                  ))}
                </div>
                <button
                  className="btn btn-primary"
                  type="button"
                  disabled={selected.size === 0}
                  onClick={() => void exportDecks(list.filter((d) => selected.has(d.id)).map((d) => d.id))}
                >
                  Export selected
                </button>
              </div>
            )}

            <div className="settings-action-row">
              <div className="settings-action-copy">
                <strong>Import backup</strong>
                <span>Restore a rem JSON backup. Same-named decks require confirmation.</span>
              </div>
              <button className="btn btn-ghost" type="button" onClick={() => importInput.current?.click()}>
                Import backup…
              </button>
              <input
                ref={importInput}
                className="settings-file-input"
                type="file"
                accept="application/json,.json"
                aria-label="Import backup file"
                onChange={onFile}
              />
            </div>

            {(error || message || pending) && (
              <div className="settings-feedback">
                {error && <p className="settings-error" role="alert">{error}</p>}
                {message && <p className="settings-status" role="status">{message}</p>}
                {pending && (
                  <div className="settings-warning" role="alertdialog" aria-label="Confirm replace">
                    <p>These decks already exist and will be replaced:</p>
                    <ul>
                      {pending.replaced.map((name) => (
                        <li key={name}>{name}</li>
                      ))}
                    </ul>
                    <div className="settings-inline-actions">
                      <button className="btn btn-danger" type="button" onClick={() => void runImport(pending.decks)}>
                        Replace
                      </button>
                      <button className="btn btn-ghost" type="button" onClick={() => setPending(null)}>
                        Cancel
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        </section>

        <section className="settings-group" aria-labelledby="sync-heading">
          <div className="settings-group-heading">
            <h2 id="sync-heading">Sync</h2>
            <p>Optional ways to keep the local collection synchronized across devices.</p>
          </div>
          <SyncSection />
        </section>
      </div>
    </>
  )
}
