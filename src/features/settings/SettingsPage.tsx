import { useEffect, useRef, useState, type ChangeEvent, type FormEvent } from 'react'
import { useStorage } from '../../data/StorageContext'
import { useSync } from '../../data/SyncContext'
import type {
  AuthProviders,
  OAuthProvider,
  SignInOutcome,
  SyncMode,
} from '../../data/SyncClient'
import { useStorageQuery } from '../../data/useStorageQuery'
import { ConfirmDialog } from '../../ui/ConfirmDialog'
import { PageHeader } from '../../ui/PageHeader'
import { SegToggle } from '../../ui/SegToggle'
import {
  applyCardTextSize,
  getStoredCardTextSize,
  type CardTextSize,
} from '../../ui/cardTextSize'
import {
  getSidebarHoverExpand,
  setSidebarHoverExpand,
} from '../../ui/sidebarPrefs'
import packageInfo from '../../../package.json'
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

function formatSyncTime(timestamp: number | null): string {
  if (!timestamp) return 'Not synced yet'
  return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function formatProvider(provider: string | null): string {
  if (!provider) return 'Cloud account'
  if (provider === 'google') return 'Google'
  if (provider === 'apple') return 'Apple'
  if (provider === 'dev_token') return 'Personal token'
  return provider
}

type AccountMismatchInfo = Extract<SignInOutcome, { status: 'accountMismatch' }>

export function SettingsPage() {
  const storage = useStorage()
  const sync = useSync()
  const decks = useStorageQuery(() => storage.listDecks(), [])

  const importInput = useRef<HTMLInputElement>(null)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [showDeckPicker, setShowDeckPicker] = useState(false)
  const [pending, setPending] = useState<{ decks: DeckBackup[]; replaced: string[] } | null>(null)
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [cardTextSize, setCardTextSize] = useState<CardTextSize>(() => getStoredCardTextSize())
  const [sidebarHoverExpand, setSidebarHoverExpandState] = useState<boolean>(() =>
    getSidebarHoverExpand(),
  )

  // Cloud sync UI state
  const [desiredMode, setDesiredMode] = useState<SyncMode>('local')
  const [serverUrl, setServerUrl] = useState('')
  const [checkedServerUrl, setCheckedServerUrl] = useState<string | null>(null)
  const [providers, setProviders] = useState<AuthProviders | null>(null)
  const [checkingServer, setCheckingServer] = useState(false)
  const [devToken, setDevToken] = useState('')
  const [syncMessage, setSyncMessage] = useState<string | null>(null)
  const [syncError, setSyncError] = useState<string | null>(null)
  const [mismatch, setMismatch] = useState<AccountMismatchInfo | null>(null)
  const [confirmDeleteAccount, setConfirmDeleteAccount] = useState(false)

  const syncStatus = sync.status

  useEffect(() => {
    if (!syncStatus) return
    if (syncStatus.mode === 'cloud') {
      setDesiredMode('cloud')
    }
    if (syncStatus.serverUrl && !serverUrl) {
      setServerUrl(syncStatus.serverUrl)
    }
  }, [syncStatus, serverUrl])

  function updateCardTextSize(next: CardTextSize) {
    applyCardTextSize(next)
    setCardTextSize(next)
  }

  function updateSidebarHoverExpand(enabled: boolean) {
    setSidebarHoverExpand(enabled)
    setSidebarHoverExpandState(enabled)
  }

  async function handleModeChange(next: SyncMode) {
    setSyncError(null)
    setSyncMessage(null)
    if (next === 'local') {
      if (syncStatus?.mode === 'cloud' && syncStatus.signedIn) {
        try {
          await sync.signOut()
          setDesiredMode('local')
          setSyncMessage('Signed out. Cloud sync paused; local cards remain on this device.')
        } catch (err) {
          setSyncError(err instanceof Error ? err.message : String(err))
        }
      } else {
        setDesiredMode('local')
      }
      return
    }
    setDesiredMode('cloud')
    if (syncStatus?.serverUrl && !serverUrl) {
      setServerUrl(syncStatus.serverUrl)
    }
  }

  async function handleCheckServer(e?: FormEvent) {
    e?.preventDefault()
    const trimmed = serverUrl.trim().replace(/\/+$/, '')
    if (!trimmed) {
      setSyncError('Enter a sync server URL first.')
      return
    }
    setSyncError(null)
    setSyncMessage(null)
    setCheckingServer(true)
    try {
      const found = await sync.fetchProviders(trimmed)
      setServerUrl(trimmed)
      setCheckedServerUrl(trimmed)
      setProviders(found)
      if (!found.google && !found.apple && !found.devToken) {
        setSyncError('This server has no sign-in providers enabled.')
      }
    } catch (err) {
      setProviders(null)
      setCheckedServerUrl(null)
      setSyncError(err instanceof Error ? err.message : String(err))
    } finally {
      setCheckingServer(false)
    }
  }

  function handleSignInOutcome(outcome: SignInOutcome) {
    if (outcome.status === 'accountMismatch') {
      setMismatch(outcome)
      return
    }
    setDevToken('')
    if (outcome.synced) {
      setSyncMessage('Connected and synced.')
    } else if (outcome.syncError) {
      setSyncError(`Connected, but initial sync failed: ${outcome.syncError}`)
    }
  }

  async function handleOAuthSignIn(provider: OAuthProvider) {
    const trimmed = serverUrl.trim().replace(/\/+$/, '')
    if (!trimmed) {
      setSyncError('Enter a sync server URL first.')
      return
    }
    setSyncError(null)
    setSyncMessage(null)
    try {
      const outcome = await sync.signInOAuth(trimmed, provider)
      handleSignInOutcome(outcome)
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

  async function handleTokenSignIn(e: FormEvent) {
    e.preventDefault()
    const trimmedUrl = serverUrl.trim().replace(/\/+$/, '')
    const trimmedToken = devToken.trim()
    if (!trimmedUrl) {
      setSyncError('Enter a sync server URL first.')
      return
    }
    if (!trimmedToken) {
      setSyncError('Enter a personal/dev token.')
      return
    }
    setSyncError(null)
    setSyncMessage(null)
    try {
      const outcome = await sync.signInToken(trimmedUrl, trimmedToken)
      handleSignInOutcome(outcome)
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

  async function handleConfirmAccountSwitch() {
    setMismatch(null)
    setSyncError(null)
    setSyncMessage(null)
    try {
      const outcome = await sync.confirmAccountSwitch()
      handleSignInOutcome(outcome)
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

  async function handleCancelAccountSwitch() {
    setMismatch(null)
    try {
      await sync.cancelAccountSwitch()
    } catch {
      // Best-effort cleanup
    }
  }

  async function handleSyncNow() {
    setSyncError(null)
    setSyncMessage(null)
    try {
      await sync.syncNow()
      setSyncMessage('Collection is up to date.')
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

  async function handleSignOut() {
    setSyncError(null)
    setSyncMessage(null)
    try {
      await sync.signOut()
      setDesiredMode('local')
      setSyncMessage('Signed out. Cloud sync paused; local cards remain on this device.')
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

  async function handleDeleteAccount() {
    setConfirmDeleteAccount(false)
    setSyncError(null)
    setSyncMessage(null)
    try {
      await sync.deleteAccount()
      setDesiredMode('local')
      setProviders(null)
      setCheckedServerUrl(null)
      setSyncMessage('Cloud account and remote data deleted. Local decks remain on this device.')
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

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

  const isCloudConnected = Boolean(syncStatus?.mode === 'cloud' && syncStatus.signedIn)
  const effectiveMode: SyncMode = isCloudConnected ? 'cloud' : desiredMode
  const serverMatchesChecked = checkedServerUrl !== null && checkedServerUrl === serverUrl.trim().replace(/\/+$/, '')

  const badgeLabel = isCloudConnected
    ? syncStatus?.activity === 'syncing'
      ? 'Syncing'
      : syncStatus?.activity === 'error'
        ? 'Error'
        : 'Active'
    : effectiveMode === 'cloud'
      ? 'Setup'
      : 'Active'

  const badgeClass =
    badgeLabel === 'Error'
      ? 'settings-badge is-error'
      : badgeLabel === 'Setup'
        ? 'settings-badge'
        : 'settings-badge is-active'

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
                  <h3>{effectiveMode === 'cloud' ? 'Cloud sync' : 'Local storage'}</h3>
                  <span className={badgeClass}>{badgeLabel}</span>
                </div>
                <p>
                  {isCloudConnected
                    ? `Signed in as ${syncStatus?.email ?? syncStatus?.accountId} via ${formatProvider(syncStatus?.provider ?? null)}.`
                    : effectiveMode === 'cloud'
                      ? 'Connect a rem sync server to keep decks, reviews, and media synced across devices.'
                      : 'Decks, review history, and media are stored privately in rem’s app data.'}
                </p>
              </div>
              {sync.supported && (
                <SegToggle
                  value={effectiveMode}
                  onChange={(next) => void handleModeChange(next)}
                  options={[
                    { value: 'local', label: 'Local only' },
                    { value: 'cloud', label: 'Cloud sync' },
                  ]}
                />
              )}
            </div>

            {effectiveMode === 'local' && syncStatus?.accountId && (
              <div className="settings-sync-note">
                Cloud sync is paused for <strong>{syncStatus.email ?? syncStatus.accountId}</strong>.
                Switch to Cloud sync to sign back in.
              </div>
            )}

            {effectiveMode === 'cloud' && sync.supported && !isCloudConnected && (
              <div className="settings-sync-setup">
                <form className="settings-sync-url-form" onSubmit={(e) => void handleCheckServer(e)}>
                  <label className="field-label" htmlFor="sync-server-url">
                    Sync server URL
                  </label>
                  <div className="settings-sync-url-row">
                    <input
                      id="sync-server-url"
                      className="text-input"
                      type="url"
                      placeholder="https://sync.example.com"
                      value={serverUrl}
                      onChange={(e) => setServerUrl(e.target.value)}
                      disabled={sync.busy || checkingServer}
                    />
                    <button
                      className="btn btn-sm"
                      type="submit"
                      disabled={sync.busy || checkingServer || serverUrl.trim().length === 0}
                    >
                      {checkingServer ? 'Checking…' : 'Continue'}
                    </button>
                  </div>
                  <p className="settings-sync-hint">
                    Point rem at your hosted or self-hosted Cloudflare Worker sync server.
                  </p>
                </form>

                {serverMatchesChecked && providers && (
                  <div className="settings-sync-providers">
                    {(providers.apple || providers.google) && (
                      <div className="settings-sync-oauth-buttons">
                        {providers.apple && (
                          <button
                            className="btn btn-sm"
                            type="button"
                            disabled={sync.busy}
                            onClick={() => void handleOAuthSignIn('apple')}
                          >
                            {sync.busy ? 'Waiting for browser…' : 'Sign in with Apple'}
                          </button>
                        )}
                        {providers.google && (
                          <button
                            className="btn btn-sm"
                            type="button"
                            disabled={sync.busy}
                            onClick={() => void handleOAuthSignIn('google')}
                          >
                            {sync.busy ? 'Waiting for browser…' : 'Sign in with Google'}
                          </button>
                        )}
                      </div>
                    )}

                    {providers.devToken && (
                      <form
                        className="settings-sync-token-form"
                        onSubmit={(e) => void handleTokenSignIn(e)}
                      >
                        <label className="field-label" htmlFor="sync-dev-token">
                          Self-hosted / personal token
                        </label>
                        <div className="settings-sync-url-row">
                          <input
                            id="sync-dev-token"
                            className="text-input"
                            type="password"
                            autoComplete="off"
                            placeholder="token or token:userId"
                            value={devToken}
                            onChange={(e) => setDevToken(e.target.value)}
                            disabled={sync.busy}
                          />
                          <button
                            className="btn btn-primary btn-sm"
                            type="submit"
                            disabled={sync.busy || devToken.trim().length === 0}
                          >
                            Connect
                          </button>
                        </div>
                      </form>
                    )}
                  </div>
                )}
              </div>
            )}

            {isCloudConnected && syncStatus && (
              <>
                <div className="settings-action-row">
                  <div className="settings-action-copy">
                    <strong>Sync status</strong>
                    <span>
                      {syncStatus.serverUrl} · Last synced: {formatSyncTime(syncStatus.lastSyncedAt)}
                      {syncStatus.pendingOutbox > 0
                        ? ` · ${syncStatus.pendingOutbox} pending`
                        : ''}
                    </span>
                  </div>
                  <div className="settings-inline-actions">
                    <button
                      className="btn btn-sm"
                      type="button"
                      disabled={sync.busy || syncStatus.activity === 'syncing'}
                      onClick={() => void handleSyncNow()}
                    >
                      {syncStatus.activity === 'syncing' ? 'Syncing…' : 'Sync now'}
                    </button>
                    <button
                      className="btn btn-ghost btn-sm"
                      type="button"
                      disabled={sync.busy}
                      onClick={() => void handleSignOut()}
                    >
                      Sign out
                    </button>
                  </div>
                </div>

                <div className="settings-action-row">
                  <div className="settings-action-copy">
                    <strong>Delete cloud account</strong>
                    <span>
                      Permanently remove your synced data and media from the server. Local cards stay
                      on this device.
                    </span>
                  </div>
                  <button
                    className="btn btn-danger-outline btn-sm"
                    type="button"
                    disabled={sync.busy}
                    onClick={() => setConfirmDeleteAccount(true)}
                  >
                    Delete cloud account…
                  </button>
                </div>
              </>
            )}

            {(syncError || syncStatus?.lastError || syncMessage) && (
              <div className="settings-feedback">
                {(syncError || syncStatus?.lastError) && (
                  <p className="settings-error" role="alert">
                    {syncError ?? syncStatus?.lastError}
                  </p>
                )}
                {syncMessage && (
                  <p className="settings-status" role="status">
                    {syncMessage}
                  </p>
                )}
              </div>
            )}

            <div className="settings-action-row">
              <div className="settings-action-copy">
                <strong>Export backup</strong>
                <span>Save a portable JSON copy of all decks or choose specific decks.</span>
              </div>
              <div className="settings-inline-actions">
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  disabled={list.length === 0}
                  aria-expanded={showDeckPicker}
                  onClick={() => setShowDeckPicker((shown) => !shown)}
                >
                  Choose decks
                </button>
                <button
                  className="btn btn-sm"
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
                  className="btn btn-primary btn-sm"
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
              <button className="btn btn-ghost btn-sm" type="button" onClick={() => importInput.current?.click()}>
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
                      <button className="btn btn-danger btn-sm" type="button" onClick={() => void runImport(pending.decks)}>
                        Replace
                      </button>
                      <button className="btn btn-ghost btn-sm" type="button" onClick={() => setPending(null)}>
                        Cancel
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        </section>

        <section className="settings-group" aria-labelledby="reading-heading">
          <div className="settings-group-heading">
            <h2 id="reading-heading">Reading &amp; interface</h2>
            <p>Tune study typography and navigation behavior to your screen.</p>
          </div>

          <div className="settings-panel">
            <div className="settings-action-row settings-action-row--first">
              <div className="settings-action-copy">
                <strong>Card text size</strong>
                <span>Adjusts question, answer, code, and editor text proportionally.</span>
              </div>
              <SegToggle
                value={cardTextSize}
                onChange={updateCardTextSize}
                options={[
                  { value: 'compact', label: 'Compact' },
                  { value: 'standard', label: 'Standard' },
                  { value: 'large', label: 'Large' },
                ]}
              />
            </div>

            <label className="settings-toggle-row settings-toggle-row--divided">
              <span>
                <strong>Auto-expand collapsed sidebar on hover</strong>
                <small>Float the full sidebar over content when hovering the collapsed icon rail.</small>
              </span>
              <input
                type="checkbox"
                checked={sidebarHoverExpand}
                onChange={(e) => updateSidebarHoverExpand(e.target.checked)}
              />
            </label>
          </div>
        </section>

        <p className="settings-version">rem version {packageInfo.version}</p>
      </div>

      {mismatch && (
        <ConfirmDialog
          title="Switch cloud sync account?"
          description={`This collection is already linked to ${mismatch.boundAccountId} (${mismatch.boundServerUrl}). Switching to ${mismatch.incomingEmail ?? mismatch.incomingAccountId} (${mismatch.incomingServerUrl}) will reset local sync history and sync your local decks to the new account. Your local cards will not be deleted.`}
          confirmLabel="Switch account"
          danger
          onConfirm={() => void handleConfirmAccountSwitch()}
          onCancel={() => void handleCancelAccountSwitch()}
        />
      )}

      {confirmDeleteAccount && (
        <ConfirmDialog
          title="Delete cloud account?"
          description="This permanently removes your synced decks, review logs, media assets, and account record from the sync server. Your local collection on this device will stay intact in Local only mode."
          confirmLabel="Delete cloud account"
          danger
          onConfirm={() => void handleDeleteAccount()}
          onCancel={() => setConfirmDeleteAccount(false)}
        />
      )}
    </>
  )
}
