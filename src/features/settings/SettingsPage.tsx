import { useEffect, useRef, useState, type ChangeEvent, type FormEvent } from 'react'
import { useStorage } from '../../data/StorageContext'
import type { ApkgImportPreview } from '../../domain/models'
import { useSync } from '../../data/SyncContext'
import { useUpdaterOptional } from '../../data/UpdaterContext'
import {
  DEFAULT_CLOUD_SERVER_URL,
  type AuthProviders,
  type EntitlementInfo,
  type EntitlementState,
  type OAuthProvider,
  type SignInOutcome,
  type SyncMode,
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

const SELF_HOSTING_GUIDE_URL = 'https://shettyh.github.io/rem/'

function AppleMark() {
  return (
    <svg className="oauth-icon" width="14" height="14" viewBox="0 0 170 170" fill="currentColor" aria-hidden="true">
      <path d="M150.37 130.25c-2.45 5.66-5.35 10.87-8.71 15.66-4.58 6.53-8.33 11.05-11.22 13.56-4.48 4.12-9.28 6.23-14.42 6.35-3.69 0-8.14-1.05-13.32-3.18-5.19-2.12-9.97-3.17-14.34-3.17-4.58 0-9.49 1.05-14.75 3.17-5.26 2.13-9.5 3.24-12.74 3.35-4.35.13-9.13-1.9-14.34-6.08-3.69-3.03-7.61-7.82-11.77-14.37-5.93-9.36-10.72-20.11-14.38-32.26-3.66-12.15-5.49-23.85-5.49-35.09 0-13.15 3.29-24.08 9.88-32.78 6.59-8.7 15.05-13.21 25.39-13.53 4.14 0 9.6 1.48 16.41 4.43 6.81 2.95 11.18 4.45 13.11 4.5 1.45 0 6.3-1.74 14.56-5.21 8.26-3.47 15.22-4.91 20.89-4.33 15.44 1.25 27.04 7.34 34.78 18.29-13.81 8.38-20.61 20.11-20.41 35.2.2 11.75 4.41 21.52 12.64 29.31 3.74 3.55 8.01 6.3 12.82 8.24-1.03 3.01-2.12 5.92-3.28 8.71zM119.22 31.01c0 9.18-3.35 17.75-10.04 25.71-8.06 9.42-17.81 14.87-28.39 14.01-.14-1.1-.21-2.27-.21-3.51 0-8.82 3.83-18.25 10.63-25.95 3.39-3.89 7.71-7.12 12.95-9.7 5.24-2.58 10.21-4 14.92-4.25.09 1.27.14 2.5.14 3.69z" />
    </svg>
  )
}

function GoogleMark() {
  return (
    <svg className="oauth-icon" width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
      <path fill="#4285F4" d="M23.49 12.27c0-.79-.07-1.55-.2-2.27H12v4.3h6.45a5.52 5.52 0 0 1-2.39 3.62v3h3.87c2.26-2.08 3.56-5.15 3.56-8.65z" />
      <path fill="#34A853" d="M12 24c3.24 0 5.95-1.07 7.93-2.91l-3.87-3c-1.07.72-2.44 1.15-4.06 1.15-3.12 0-5.77-2.11-6.72-4.95H1.28v3.1A12 12 0 0 0 12 24z" />
      <path fill="#FBBC05" d="M5.28 14.29A7.2 7.2 0 0 1 4.9 12c0-.8.14-1.57.38-2.29V6.61H1.28A12 12 0 0 0 0 12c0 1.94.46 3.77 1.28 5.39l4-3.1z" />
      <path fill="#EA4335" d="M12 4.76c1.76 0 3.34.61 4.58 1.8l3.44-3.44C17.95 1.19 15.24 0 12 0A12 12 0 0 0 1.28 6.61l4 3.1c.95-2.84 3.6-4.95 6.72-4.95z" />
    </svg>
  )
}

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

function daysRemaining(expiresAt: number | null): number | null {
  if (!expiresAt) return null
  return Math.max(1, Math.ceil((expiresAt - Date.now()) / 86_400_000))
}

function formatShortDate(timestamp: number | null): string | null {
  if (!timestamp) return null
  return new Date(timestamp).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  })
}

function describeEntitlementBadge(
  entitlement: EntitlementInfo | null,
  entState: EntitlementState,
  activity: 'idle' | 'syncing' | 'error',
): { label: string; className: string } {
  if (activity === 'syncing') {
    return { label: 'Syncing', className: 'settings-badge is-active' }
  }
  const days = daysRemaining(entitlement?.expiresAt ?? null)
  switch (entState) {
    case 'trialing':
      return {
        label: days ? `Trial · ${days}d left` : 'Trial',
        className: 'settings-badge is-active',
      }
    case 'active':
      return { label: 'Active', className: 'settings-badge is-active' }
    case 'past_due':
      return { label: 'Payment issue', className: 'settings-badge is-warn' }
    case 'expired':
      return { label: 'Sync paused', className: 'settings-badge is-warn' }
    case 'none':
      return { label: 'Not active', className: 'settings-badge is-warn' }
    case 'self_hosted':
      if (activity === 'error') {
        return { label: 'Error', className: 'settings-badge is-error' }
      }
      return { label: 'Active', className: 'settings-badge is-active' }
  }
}

type AccountMismatchInfo = Extract<SignInOutcome, { status: 'accountMismatch' }>

export function SettingsPage() {
  const storage = useStorage()
  const sync = useSync()
  const updater = useUpdaterOptional()
  const decks = useStorageQuery(() => storage.listDecks(), [])

  const importInput = useRef<HTMLInputElement>(null)
  const apkgInput = useRef<HTMLInputElement>(null)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [showDeckPicker, setShowDeckPicker] = useState(false)
  const [pending, setPending] = useState<{ decks: DeckBackup[]; replaced: string[] } | null>(null)
  const [pendingApkg, setPendingApkg] = useState<{ bytes: Uint8Array; preview: ApkgImportPreview } | null>(null)
  const [importingApkg, setImportingApkg] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [cardTextSize, setCardTextSize] = useState<CardTextSize>(() => getStoredCardTextSize())
  const [sidebarHoverExpand, setSidebarHoverExpandState] = useState<boolean>(() =>
    getSidebarHoverExpand(),
  )

  // Cloud sync UI state
  const [desiredMode, setDesiredMode] = useState<SyncMode>('local')
  const [useCustomServer, setUseCustomServer] = useState(false)
  const [serverUrl, setServerUrl] = useState('')
  const [checkedServerUrl, setCheckedServerUrl] = useState<string | null>(null)
  const [providers, setProviders] = useState<AuthProviders | null>(null)
  const [checkingServer, setCheckingServer] = useState(false)
  const [hostedProviders, setHostedProviders] = useState<AuthProviders | null>(null)
  const [hostedCheckedUrl, setHostedCheckedUrl] = useState<string | null>(null)
  const [checkingHosted, setCheckingHosted] = useState(false)
  const [hostedError, setHostedError] = useState<string | null>(null)
  const [devToken, setDevToken] = useState('')
  const [syncMessage, setSyncMessage] = useState<string | null>(null)
  const [syncError, setSyncError] = useState<string | null>(null)
  const [mismatch, setMismatch] = useState<AccountMismatchInfo | null>(null)
  const [confirmDeleteAccount, setConfirmDeleteAccount] = useState(false)
  const [awaitingCheckoutReturn, setAwaitingCheckoutReturn] = useState(false)

  const syncStatus = sync.status
  const defaultServerUrl = (syncStatus?.defaultServerUrl ?? DEFAULT_CLOUD_SERVER_URL).replace(
    /\/+$/,
    '',
  )
  const isCloudConnected = Boolean(syncStatus?.mode === 'cloud' && syncStatus.signedIn)
  const effectiveMode: SyncMode = isCloudConnected ? 'cloud' : desiredMode

  useEffect(() => {
    if (!syncStatus) return
    if (syncStatus.mode === 'cloud') {
      setDesiredMode('cloud')
    }
    if (syncStatus.serverUrl) {
      const boundNormalized = syncStatus.serverUrl.replace(/\/+$/, '')
      if (boundNormalized !== defaultServerUrl) {
        setUseCustomServer(true)
        if (!serverUrl) setServerUrl(boundNormalized)
      }
    }
  }, [syncStatus, defaultServerUrl, serverUrl])

  const fetchProviders = sync.fetchProviders
  useEffect(() => {
    if (
      effectiveMode !== 'cloud' ||
      !sync.supported ||
      isCloudConnected ||
      useCustomServer ||
      hostedCheckedUrl === defaultServerUrl
    ) {
      return
    }
    let cancelled = false
    setCheckingHosted(true)
    setHostedError(null)
    void fetchProviders(defaultServerUrl)
      .then((found) => {
        if (cancelled) return
        setHostedProviders(found)
        setHostedCheckedUrl(defaultServerUrl)
        setCheckingHosted(false)
        if (!found.google && !found.apple) {
          setHostedError('rem Cloud sign-in is not currently available.')
        }
      })
      .catch((err) => {
        if (cancelled) return
        setHostedProviders(null)
        setHostedCheckedUrl(defaultServerUrl)
        setCheckingHosted(false)
        setHostedError(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [
    effectiveMode,
    sync.supported,
    fetchProviders,
    isCloudConnected,
    useCustomServer,
    hostedCheckedUrl,
    defaultServerUrl,
  ])

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
      if (!isCloudConnected) {
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
    } else if (outcome.snapshot.subscriptionRequired) {
      setSyncMessage(null)
      setSyncError(null)
    } else if (outcome.syncError) {
      setSyncError(`Connected, but initial sync failed: ${outcome.syncError}`)
    }
  }

  async function handleOAuthSignIn(provider: OAuthProvider, targetServerUrl?: string) {
    const raw = targetServerUrl ?? (useCustomServer ? serverUrl : defaultServerUrl)
    const trimmed = raw.trim().replace(/\/+$/, '')
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

  async function handleCheckAccess() {
    setSyncError(null)
    setSyncMessage(null)
    try {
      const refreshed = await sync.refreshAccount()
      const enabled = refreshed.entitlement
        ? refreshed.entitlement.syncEnabled
        : !refreshed.subscriptionRequired
      if (enabled) {
        await sync.syncNow()
        setSyncMessage('Access confirmed. Collection is up to date.')
      } else {
        setSyncMessage('Checked account status. Cloud sync is still paused.')
      }
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

  async function handleStartTrial() {
    setSyncError(null)
    setSyncMessage(null)
    try {
      const next = await sync.startTrial()
      const days = next.entitlement?.trialDays ?? 30
      setSyncMessage(`Your ${days}-day free trial is active and syncing.`)
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

  async function handleOpenPolarCheckout() {
    setSyncError(null)
    setSyncMessage(null)
    try {
      await sync.openPolarCheckout()
      setAwaitingCheckoutReturn(true)
      setSyncMessage(
        'Opened Polar checkout in your browser. Return to rem after subscribing — sync updates automatically.',
      )
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : String(err))
    }
  }

  async function handleOpenPolarPortal() {
    setSyncError(null)
    setSyncMessage(null)
    try {
      await sync.openPolarPortal()
      setSyncMessage('Opened billing portal in your browser.')
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
    setPendingApkg(null)
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

  async function runApkgImport(bytes: Uint8Array) {
    if (!storage.importApkg) return
    setImportingApkg(true)
    setError(null)
    setMessage(null)
    try {
      const result = await storage.importApkg(bytes)
      setPendingApkg(null)
      const deckTotal = result.added.length + result.replaced.length
      const parts = [
        `${result.importedCards} card${result.importedCards === 1 ? '' : 's'}`,
        `${deckTotal} deck${deckTotal === 1 ? '' : 's'}`,
      ]
      if (result.importedReviews > 0) {
        parts.push(`${result.importedReviews} review${result.importedReviews === 1 ? '' : 's'}`)
      }
      if (result.importedMedia > 0) {
        parts.push(`${result.importedMedia} media`)
      }
      const replacedNote = result.replaced.length ? ` (replaced ${result.replaced.length})` : ''
      const warningNote = result.warnings.length ? ` · Warnings: ${result.warnings.join(' ')}` : ''
      setMessage(`Imported Anki deck: ${parts.join(', ')}${replacedNote}.${warningNote}`)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not import the Anki archive.')
    } finally {
      setImportingApkg(false)
    }
  }

  async function onApkgFile(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file || !storage.previewApkg || !storage.importApkg) return
    setError(null)
    setMessage(null)
    setPending(null)
    setPendingApkg(null)
    setPendingApkg(null)
    if (file.size > 512 * 1024 * 1024) {
      setError('Anki archive exceeds the maximum supported size (512 MB).')
      return
    }
    setImportingApkg(true)
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      const preview = await storage.previewApkg(bytes)
      if (preview.totalCards === 0 && preview.added.length === 0 && preview.replaced.length === 0) {
        const detail = preview.warnings.length > 0 ? ` (${preview.warnings[0]})` : ''
        setError(`No importable cards were found in this .apkg file.${detail}`)
      } else if (preview.replaced.length > 0) {
        setPendingApkg({ bytes, preview })
      } else {
        await runApkgImport(bytes)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not read the Anki archive.')
    } finally {
      setImportingApkg(false)
    }
  }

  const serverMatchesChecked = checkedServerUrl !== null && checkedServerUrl === serverUrl.trim().replace(/\/+$/, '')
  const isDefaultConnectedServer =
    syncStatus?.serverUrl?.replace(/\/+$/, '') === defaultServerUrl

  const entitlement = syncStatus?.entitlement ?? null
  const entState: EntitlementState =
    entitlement?.state ?? (syncStatus?.subscriptionRequired ? 'none' : 'self_hosted')
  const isSyncPaused = entitlement ? !entitlement.syncEnabled : Boolean(syncStatus?.subscriptionRequired)
  const trialDays = entitlement?.trialDays ?? 30
  const priceDisplay = entitlement?.priceDisplay ?? '$5/year'
  const daysLeft = daysRemaining(entitlement?.expiresAt ?? null)
  const periodEndFormatted = formatShortDate(entitlement?.expiresAt ?? null)

  useEffect(() => {
    if (!awaitingCheckoutReturn) return
    if (entitlement?.state === 'active') {
      setAwaitingCheckoutReturn(false)
      setSyncError(null)
      setSyncMessage('rem Premium is active and syncing.')
    }
  }, [awaitingCheckoutReturn, entitlement?.state])

  const connectedBadge = syncStatus
    ? describeEntitlementBadge(entitlement, entState, syncStatus.activity)
    : null
  const badgeLabel = isCloudConnected
    ? connectedBadge!.label
    : effectiveMode === 'cloud'
      ? 'Setup'
      : 'Active'
  const badgeClass = isCloudConnected
    ? connectedBadge!.className
    : effectiveMode === 'cloud'
      ? 'settings-badge'
      : 'settings-badge is-active'

  const syncStatusDescription = (() => {
    if (!syncStatus) return ''
    if (isSyncPaused) {
      return 'Sync paused · Your local cards remain safe on this device.'
    }
    const syncTail = `Last synced: ${formatSyncTime(syncStatus.lastSyncedAt)}${
      syncStatus.pendingOutbox > 0 ? ` · ${syncStatus.pendingOutbox} pending` : ''
    }`
    switch (entState) {
      case 'trialing':
        return `rem Premium · Trial (${daysLeft ?? trialDays}d left) · ${syncTail}`
      case 'active': {
        const via =
          entitlement?.provider === 'apple' ? 'Managed via Apple App Store' : 'rem Premium'
        const renewal = periodEndFormatted
          ? ` · ${entitlement?.cancelAtPeriodEnd ? 'Ends' : 'Renews'} ${periodEndFormatted}`
          : ''
        return `${via}${renewal} · ${syncTail}`
      }
      case 'past_due':
        return `rem Premium · Grace period active · ${syncTail}`
      default:
        return `${isDefaultConnectedServer ? 'rem Cloud' : syncStatus.serverUrl} · ${syncTail}`
    }
  })()

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
                    ? `Signed in as ${syncStatus?.email ?? formatProvider(syncStatus?.provider ?? null)} via ${formatProvider(syncStatus?.provider ?? null)}.`
                    : effectiveMode === 'cloud'
                      ? 'Sync decks, study progress, and images across all your devices with rem Premium.'
                      : 'Decks, study progress, and images are stored privately on this device.'}
                </p>
              </div>
              {sync.supported && !isCloudConnected && (
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
                Cloud sync is paused{syncStatus.email ? <> for <strong>{syncStatus.email}</strong></> : null}.
                Switch to Cloud sync to sign back in.
              </div>
            )}

            {effectiveMode === 'cloud' && sync.supported && !isCloudConnected && !useCustomServer && (
              <div className="settings-sync-setup">
                <div className="settings-sync-hosted">
                  <p className="settings-sync-hosted-copy">
                    All local features and self-hosting stay free forever. Sign in to start a
                    30-day free trial (no card required) or connect an existing rem Premium account.
                  </p>

                  {checkingHosted && (
                    <p className="settings-sync-hint">Checking rem Premium sign-in options…</p>
                  )}

                  {!checkingHosted && hostedError && (
                    <div className="settings-inline-actions">
                      <p className="settings-error" role="alert">
                        {hostedError}
                      </p>
                      <button
                        className="btn btn-sm"
                        type="button"
                        onClick={() => setHostedCheckedUrl(null)}
                      >
                        Retry
                      </button>
                    </div>
                  )}

                  {!checkingHosted && hostedProviders && (hostedProviders.apple || hostedProviders.google) && (
                    <div className="settings-sync-oauth-buttons">
                      {hostedProviders.apple && (
                        <button
                          className="btn btn-sm btn-oauth"
                          type="button"
                          disabled={sync.busy}
                          onClick={() => void handleOAuthSignIn('apple', defaultServerUrl)}
                        >
                          <AppleMark />
                          <span>{sync.busy ? 'Waiting for browser…' : 'Sign in with Apple'}</span>
                        </button>
                      )}
                      {hostedProviders.google && (
                        <button
                          className="btn btn-sm btn-oauth"
                          type="button"
                          disabled={sync.busy}
                          onClick={() => void handleOAuthSignIn('google', defaultServerUrl)}
                        >
                          <GoogleMark />
                          <span>{sync.busy ? 'Waiting for browser…' : 'Sign in with Google'}</span>
                        </button>
                      )}
                    </div>
                  )}
                </div>

                <div className="settings-sync-mode-switch">
                  <button
                    className="settings-sync-text-btn"
                    type="button"
                    onClick={() => {
                      setSyncError(null)
                      setUseCustomServer(true)
                    }}
                  >
                    Use a custom server
                  </button>
                </div>
              </div>
            )}

            {effectiveMode === 'cloud' && sync.supported && !isCloudConnected && useCustomServer && (
              <div className="settings-sync-setup">
                <form className="settings-sync-url-form" onSubmit={(e) => void handleCheckServer(e)}>
                  <label className="field-label" htmlFor="sync-server-url">
                    Custom sync server URL
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
                    Connect to your own sync server.{' '}
                    <a
                      className="settings-sync-guide-link"
                      href={SELF_HOSTING_GUIDE_URL}
                      target="_blank"
                      rel="noreferrer"
                    >
                      Setup guide ↗
                    </a>
                  </p>
                </form>

                {serverMatchesChecked && providers && (
                  <div className="settings-sync-providers">
                    {(providers.apple || providers.google) && (
                      <div className="settings-sync-oauth-buttons">
                        {providers.apple && (
                          <button
                            className="btn btn-sm btn-oauth"
                            type="button"
                            disabled={sync.busy}
                            onClick={() => void handleOAuthSignIn('apple')}
                          >
                            <AppleMark />
                            <span>{sync.busy ? 'Waiting for browser…' : 'Sign in with Apple'}</span>
                          </button>
                        )}
                        {providers.google && (
                          <button
                            className="btn btn-sm btn-oauth"
                            type="button"
                            disabled={sync.busy}
                            onClick={() => void handleOAuthSignIn('google')}
                          >
                            <GoogleMark />
                            <span>{sync.busy ? 'Waiting for browser…' : 'Sign in with Google'}</span>
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
                          Personal access token
                        </label>
                        <div className="settings-sync-url-row">
                          <input
                            id="sync-dev-token"
                            className="text-input"
                            type="password"
                            autoComplete="off"
                            placeholder="Paste your access token"
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

                <div className="settings-sync-mode-switch">
                  <button
                    className="settings-sync-text-btn"
                    type="button"
                    onClick={() => {
                      setSyncError(null)
                      setUseCustomServer(false)
                    }}
                  >
                    Use rem Cloud instead
                  </button>
                </div>
              </div>
            )}

            {isCloudConnected && syncStatus && (
              <>
                {(entState === 'none' ||
                  entState === 'trialing' ||
                  entState === 'past_due' ||
                  entState === 'expired') && (
                  <div className="settings-sync-note settings-sync-plan-note" role="status">
                    <div className="settings-sync-plan-header">
                      <div>
                        <strong>
                          {entState === 'none'
                            ? 'Activate rem Premium'
                            : entState === 'trialing'
                              ? `Free trial active · ${daysLeft ?? trialDays} day${(daysLeft ?? trialDays) === 1 ? '' : 's'} left`
                              : entState === 'past_due'
                                ? 'Payment issue'
                                : 'Sync paused'}
                        </strong>
                        <p>
                          {entState === 'none'
                            ? `All local features and self-hosting stay free forever. Start a ${trialDays}-day free trial (no card required) or subscribe to rem Premium.`
                            : entState === 'trialing'
                              ? 'Your collection is syncing across all devices. You can upgrade to rem Premium anytime.'
                              : entState === 'past_due'
                                ? entitlement?.provider === 'apple'
                                  ? 'Your App Store subscription needs attention. Update payment details in Apple Subscriptions. Local cards stay safe on this device.'
                                  : 'We could not renew your rem Premium plan. Update billing details to keep syncing. Local cards stay safe on this device.'
                                : 'Your trial or rem Premium plan has ended. Your local cards stay on this device.'}
                        </p>
                      </div>

                      <div className="settings-inline-actions">
                        {(entState === 'none' || entState === 'expired') &&
                          (entitlement ? entitlement.canStartTrial : entState === 'none') && (
                            <button
                              className="btn btn-primary btn-sm"
                              type="button"
                              disabled={sync.busy}
                              onClick={() => void handleStartTrial()}
                            >
                              Start {trialDays}-day free trial
                            </button>
                          )}

                        {entitlement?.canSubscribePolar && (
                          <button
                            className="btn btn-sm"
                            type="button"
                            disabled={sync.busy}
                            onClick={() => void handleOpenPolarCheckout()}
                          >
                            Upgrade to Premium — {priceDisplay}
                          </button>
                        )}

                        {entState === 'past_due' && entitlement?.canManageBilling && (
                          <button
                            className="btn btn-primary btn-sm"
                            type="button"
                            disabled={sync.busy}
                            onClick={() => void handleOpenPolarPortal()}
                          >
                            Manage billing ↗
                          </button>
                        )}

                        {(entState === 'none' || entState === 'past_due' || entState === 'expired') && (
                          <button
                            className="btn btn-ghost btn-sm"
                            type="button"
                            disabled={sync.busy}
                            onClick={() => void handleCheckAccess()}
                          >
                            Check access / Retry sync
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                )}

                <div className="settings-action-row">
                  <div className="settings-action-copy">
                    <strong>Sync status</strong>
                    <span>{syncStatusDescription}</span>
                  </div>
                  <div className="settings-inline-actions">
                    {!isSyncPaused && (
                      <button
                        className="btn btn-sm"
                        type="button"
                        disabled={sync.busy || syncStatus.activity === 'syncing'}
                        onClick={() => void handleSyncNow()}
                      >
                        {syncStatus.activity === 'syncing' ? 'Syncing…' : 'Sync now'}
                      </button>
                    )}
                    {entState === 'active' && entitlement?.canManageBilling && (
                      <button
                        className="btn btn-ghost btn-sm"
                        type="button"
                        disabled={sync.busy}
                        onClick={() => void handleOpenPolarPortal()}
                      >
                        Manage billing ↗
                      </button>
                    )}
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

            {(syncError || (!isSyncPaused && syncStatus?.lastError) || syncMessage) && (
              <div className="settings-feedback">
                {(syncError || (!isSyncPaused && syncStatus?.lastError)) && (
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
                <span>Save a backup copy of all decks or choose specific decks.</span>
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
                <span>Restore a rem backup file. Same-named decks require confirmation.</span>
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

            {storage.importApkg && storage.previewApkg && (
              <div className="settings-action-row">
                <div className="settings-action-copy">
                  <strong>Import Anki deck (.apkg)</strong>
                  <span>
                    Import decks, cloze cards, LaTeX, images, audio, and review history from Anki.
                  </span>
                </div>
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  disabled={importingApkg}
                  onClick={() => apkgInput.current?.click()}
                >
                  {importingApkg ? 'Importing…' : 'Import .apkg…'}
                </button>
                <input
                  ref={apkgInput}
                  className="settings-file-input"
                  type="file"
                  accept=".apkg,application/octet-stream,application/zip"
                  aria-label="Import Anki deck file"
                  onChange={onApkgFile}
                />
              </div>
            )}

            {(error || message || pending || pendingApkg) && (
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
                {pendingApkg && (
                  <div className="settings-warning" role="alertdialog" aria-label="Confirm Anki replace">
                    <p>
                      Importing {pendingApkg.preview.totalCards} card
                      {pendingApkg.preview.totalCards === 1 ? '' : 's'}
                      {pendingApkg.preview.mediaCount > 0 ? `, ${pendingApkg.preview.mediaCount} media` : ''}
                      {pendingApkg.preview.reviewLogs > 0 ? `, ${pendingApkg.preview.reviewLogs} review logs` : ''}
                      {pendingApkg.preview.added.length > 0
                        ? ` (adding ${pendingApkg.preview.added.length} new deck${pendingApkg.preview.added.length === 1 ? '' : 's'})`
                        : ''}
                      {' '}will replace these existing decks:
                    </p>
                    <ul>
                      {pendingApkg.preview.replaced.map((name) => (
                        <li key={name}>{name}</li>
                      ))}
                    </ul>
                    {pendingApkg.preview.warnings.length > 0 && (
                      <p className="ds-row-sub">
                        Warnings: {pendingApkg.preview.warnings.join(' · ')}
                      </p>
                    )}
                    <div className="settings-inline-actions">
                      <button
                        className="btn btn-danger btn-sm"
                        type="button"
                        disabled={importingApkg}
                        onClick={() => void runApkgImport(pendingApkg.bytes)}
                      >
                        {importingApkg ? 'Importing…' : 'Replace & import'}
                      </button>
                      <button
                        className="btn btn-ghost btn-sm"
                        type="button"
                        disabled={importingApkg}
                        onClick={() => setPendingApkg(null)}
                      >
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

        {updater?.supported && (
          <section className="settings-group" aria-labelledby="app-updates-heading">
            <div className="settings-group-heading">
              <h2 id="app-updates-heading">App &amp; updates</h2>
              <p>Keep rem and its bundled terminal CLI up to date.</p>
            </div>

            <div className="settings-panel">
              <div className="settings-action-row settings-action-row--first">
                <div className="settings-action-copy">
                  <strong>Software updates</strong>
                  <span>
                    {updater.state.status === 'checking' && 'Checking for updates…'}
                    {updater.state.status === 'idle' && `Installed version v${packageInfo.version}.`}
                    {updater.state.status === 'up-to-date' &&
                      `You're up to date on v${packageInfo.version}.`}
                    {updater.state.status === 'available' &&
                      `Version v${updater.state.info.version} is available.`}
                    {updater.state.status === 'downloading' &&
                      (updater.state.progress.total
                        ? `Downloading v${updater.state.info.version} (${Math.min(
                            100,
                            Math.round(
                              (updater.state.progress.downloaded / updater.state.progress.total) * 100,
                            ),
                          )}%)…`
                        : `Downloading v${updater.state.info.version}…`)}
                    {updater.state.status === 'ready' &&
                      `Version v${updater.state.info.version} is ready. Restart to finish updating.`}
                    {updater.state.status === 'error' && `Installed version v${packageInfo.version}.`}
                  </span>
                  {updater.state.status === 'downloading' && updater.state.progress.total && (
                    <div className="settings-progress" role="progressbar" aria-label="Update download progress">
                      <div
                        className="settings-progress-fill"
                        style={{
                          width: `${Math.min(
                            100,
                            Math.round(
                              (updater.state.progress.downloaded / updater.state.progress.total) * 100,
                            ),
                          )}%`,
                        }}
                      />
                    </div>
                  )}
                  {(updater.state.status === 'available' || updater.state.status === 'ready') &&
                    updater.state.info.body && (
                      <pre className="settings-update-notes">{updater.state.info.body}</pre>
                    )}
                </div>

                <div className="settings-inline-actions">
                  {updater.state.status === 'available' && (
                    <button
                      className="btn btn-primary btn-sm"
                      type="button"
                      onClick={() => void updater.downloadAndInstall()}
                    >
                      Install v{updater.state.info.version}
                    </button>
                  )}
                  {updater.state.status === 'ready' && (
                    <button
                      className="btn btn-primary btn-sm"
                      type="button"
                      disabled={updater.relaunching}
                      onClick={() => void updater.relaunch()}
                    >
                      {updater.relaunching ? 'Restarting…' : 'Restart to update'}
                    </button>
                  )}
                  {updater.state.status !== 'available' &&
                    updater.state.status !== 'downloading' &&
                    updater.state.status !== 'ready' && (
                      <button
                        className="btn btn-ghost btn-sm"
                        type="button"
                        disabled={updater.state.status === 'checking'}
                        onClick={() => void updater.checkForUpdates()}
                      >
                        {updater.state.status === 'checking' ? 'Checking…' : 'Check for updates'}
                      </button>
                    )}
                </div>
              </div>

              {updater.state.status === 'error' && (
                <div className="settings-feedback">
                  <p className="settings-error" role="alert">
                    {updater.state.message}
                  </p>
                </div>
              )}

              <label className="settings-toggle-row settings-toggle-row--divided">
                <span>
                  <strong>Automatic updates</strong>
                  <small>
                    Check for and download updates in the background, then prompt when ready to
                    restart.
                  </small>
                </span>
                <input
                  type="checkbox"
                  checked={updater.autoUpdateEnabled}
                  onChange={(e) => updater.setAutoUpdateEnabled(e.target.checked)}
                />
              </label>

              {updater.cliStatus?.supported && (
                <div className="settings-action-row">
                  <div className="settings-action-copy">
                    <strong>Command line (rem)</strong>
                    <span>
                      {updater.cliStatus.linkedToBundled
                        ? updater.cliStatus.managedByHomebrew
                          ? `Linked via Homebrew at ${updater.cliStatus.installedPath}. Updates automatically with rem.`
                          : updater.cliStatus.onPath
                            ? `Linked at ${updater.cliStatus.installedPath}. Updates automatically with rem.`
                            : `Linked at ${updater.cliStatus.installedPath}. Add ~/.local/bin to your PATH to run rem.`
                        : updater.cliStatus.installedPath
                          ? `Found separate CLI at ${updater.cliStatus.installedPath}. Link the bundled binary so updates stay in sync.`
                          : 'Link the bundled rem CLI into ~/.local/bin so you can capture and study from your terminal.'}
                    </span>
                  </div>
                  {!updater.cliStatus.linkedToBundled && (
                    <button
                      className="btn btn-ghost btn-sm"
                      type="button"
                      disabled={updater.cliBusy}
                      onClick={() => void updater.installCli()}
                    >
                      {updater.cliBusy ? 'Linking…' : 'Install rem CLI'}
                    </button>
                  )}
                </div>
              )}

              {updater.cliError && (
                <div className="settings-feedback">
                  <p className="settings-error" role="alert">
                    {updater.cliError}
                  </p>
                </div>
              )}
            </div>
          </section>
        )}

        <p className="settings-version">rem version {packageInfo.version}</p>
      </div>

      {mismatch && (
        <ConfirmDialog
          title="Switch cloud sync account?"
          description={`This device is already linked to another cloud account. Switching to ${mismatch.incomingEmail ?? 'the new account'} will reset local sync history and sync your local decks to it. Your local cards will not be deleted.`}
          confirmLabel="Switch account"
          danger
          onConfirm={() => void handleConfirmAccountSwitch()}
          onCancel={() => void handleCancelAccountSwitch()}
        />
      )}

      {confirmDeleteAccount && (
        <ConfirmDialog
          title="Delete cloud account?"
          description={
            entState === 'active' || entState === 'past_due'
              ? entitlement?.provider === 'apple'
                ? 'This permanently removes your synced decks, review logs, media assets, and account record from the sync server. Your local collection stays intact. Remember to cancel your App Store subscription in Apple Subscriptions to prevent renewal.'
                : 'This permanently removes your synced decks, review logs, media assets, and account record from the sync server, and cancels your active rem Premium web subscription. Your local collection on this device will stay intact in Local only mode.'
              : 'This permanently removes your synced decks, review logs, media assets, and account record from the sync server. Your local collection on this device will stay intact in Local only mode.'
          }
          confirmLabel="Delete cloud account"
          danger
          onConfirm={() => void handleDeleteAccount()}
          onCancel={() => setConfirmDeleteAccount(false)}
        />
      )}
    </>
  )
}
