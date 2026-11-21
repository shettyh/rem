import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { useStorage } from './StorageContext'
import {
  defaultSyncClient,
  type AuthProviders,
  type OAuthProvider,
  type SignInOutcome,
  type SyncClient,
  type SyncStatus,
} from './SyncClient'

const WRITE_DEBOUNCE_MS = 3_000
const FOCUS_SYNC_THROTTLE_MS = 30_000
const PERIODIC_SYNC_MS = 5 * 60_000

export interface SyncContextValue {
  supported: boolean
  status: SyncStatus | null
  busy: boolean
  refreshStatus: () => Promise<SyncStatus | null>
  fetchProviders: (serverUrl: string) => Promise<AuthProviders>
  signInOAuth: (serverUrl: string, provider: OAuthProvider) => Promise<SignInOutcome>
  signInToken: (serverUrl: string, token: string) => Promise<SignInOutcome>
  confirmAccountSwitch: () => Promise<SignInOutcome>
  cancelAccountSwitch: () => Promise<void>
  syncNow: () => Promise<SyncStatus | null>
  signOut: () => Promise<SyncStatus>
  deleteAccount: () => Promise<SyncStatus>
}

const SyncContext = createContext<SyncContextValue | null>(null)

export function SyncProvider({
  children,
  client = defaultSyncClient,
  autoSync = true,
}: {
  children: ReactNode
  client?: SyncClient
  /** Disable background timers in deterministic UI tests when desired. */
  autoSync?: boolean
}) {
  const storage = useStorage()
  const [status, setStatus] = useState<SyncStatus | null>(null)
  const [busy, setBusy] = useState(false)

  const statusRef = useRef<SyncStatus | null>(null)
  const syncingRef = useRef(false)
  const skipLocalNotifyRef = useRef(false)
  const lastAutoSyncAtRef = useRef(0)
  const didStartupSyncRef = useRef(false)

  const updateStatus = useCallback((next: SyncStatus) => {
    statusRef.current = next
    setStatus(next)
  }, [])

  const refreshStatus = useCallback(async () => {
    try {
      const next = await client.getStatus()
      updateStatus(next)
      return next
    } catch {
      return null
    }
  }, [client, updateStatus])

  const triggerSync = useCallback(
    async (silent: boolean) => {
      if (!client.supported || syncingRef.current) return statusRef.current
      syncingRef.current = true
      if (!silent) setBusy(true)
      try {
        const next = await client.syncNow()
        updateStatus(next)
        lastAutoSyncAtRef.current = Date.now()
        return next
      } catch (err) {
        await refreshStatus()
        if (!silent) throw err
        return null
      } finally {
        syncingRef.current = false
        if (!silent) setBusy(false)
      }
    },
    [client, refreshStatus, updateStatus],
  )

  useEffect(() => {
    let active = true
    void client.getStatus().then((initial) => {
      if (!active) return
      updateStatus(initial)
      if (
        autoSync &&
        client.supported &&
        initial.mode === 'cloud' &&
        initial.signedIn &&
        !initial.subscriptionRequired &&
        !didStartupSyncRef.current
      ) {
        didStartupSyncRef.current = true
        void triggerSync(true)
      }
    })

    const unsubStatus = client.subscribeStatus((next) => {
      if (active) updateStatus(next)
    })
    const unsubRemote = client.subscribeRemoteApplied((appliedOps) => {
      if (!active || appliedOps <= 0) return
      skipLocalNotifyRef.current = true
      storage.notifyExternalChange?.()
      skipLocalNotifyRef.current = false
    })

    return () => {
      active = false
      unsubStatus()
      unsubRemote()
    }
  }, [autoSync, client, storage, triggerSync, updateStatus])

  // Debounced sync after local mutations + periodic & focus sync when Cloud sync is active.
  useEffect(() => {
    if (!autoSync || !client.supported) return
    let debounceTimer: number | null = null

    const scheduleLocalSync = () => {
      if (skipLocalNotifyRef.current) return
      const current = statusRef.current
      if (!current || current.mode !== 'cloud' || !current.signedIn || current.subscriptionRequired) return
      if (debounceTimer !== null) window.clearTimeout(debounceTimer)
      debounceTimer = window.setTimeout(() => {
        debounceTimer = null
        void triggerSync(true)
      }, WRITE_DEBOUNCE_MS)
    }

    const unsubStorage = storage.subscribe(scheduleLocalSync)

    const onWake = () => {
      const current = statusRef.current
      if (!current || current.mode !== 'cloud' || !current.signedIn || current.subscriptionRequired) return
      if (Date.now() - lastAutoSyncAtRef.current < FOCUS_SYNC_THROTTLE_MS) return
      void triggerSync(true)
    }

    window.addEventListener('focus', onWake)
    window.addEventListener('online', onWake)
    const interval = window.setInterval(() => {
      const current = statusRef.current
      if (!current || current.mode !== 'cloud' || !current.signedIn || current.subscriptionRequired) return
      void triggerSync(true)
    }, PERIODIC_SYNC_MS)

    return () => {
      unsubStorage()
      if (debounceTimer !== null) window.clearTimeout(debounceTimer)
      window.removeEventListener('focus', onWake)
      window.removeEventListener('online', onWake)
      window.clearInterval(interval)
    }
  }, [autoSync, client.supported, storage, triggerSync])

  const fetchProviders = useCallback(
    (serverUrl: string) => client.fetchProviders(serverUrl),
    [client],
  )

  const signInOAuth = useCallback(
    async (serverUrl: string, provider: OAuthProvider) => {
      setBusy(true)
      try {
        const outcome = await client.signInOAuth(serverUrl, provider)
        if (outcome.status === 'connected') updateStatus(outcome.snapshot)
        return outcome
      } finally {
        setBusy(false)
      }
    },
    [client, updateStatus],
  )

  const signInToken = useCallback(
    async (serverUrl: string, token: string) => {
      setBusy(true)
      try {
        const outcome = await client.signInToken(serverUrl, token)
        if (outcome.status === 'connected') updateStatus(outcome.snapshot)
        return outcome
      } finally {
        setBusy(false)
      }
    },
    [client, updateStatus],
  )

  const confirmAccountSwitch = useCallback(async () => {
    setBusy(true)
    try {
      const outcome = await client.confirmAccountSwitch()
      if (outcome.status === 'connected') updateStatus(outcome.snapshot)
      return outcome
    } finally {
      setBusy(false)
    }
  }, [client, updateStatus])

  const cancelAccountSwitch = useCallback(() => client.cancelAccountSwitch(), [client])

  const syncNow = useCallback(() => triggerSync(false), [triggerSync])

  const signOut = useCallback(async () => {
    setBusy(true)
    try {
      const next = await client.signOut()
      updateStatus(next)
      return next
    } finally {
      setBusy(false)
    }
  }, [client, updateStatus])

  const deleteAccount = useCallback(async () => {
    setBusy(true)
    try {
      const next = await client.deleteAccount()
      updateStatus(next)
      return next
    } finally {
      setBusy(false)
    }
  }, [client, updateStatus])

  const value = useMemo<SyncContextValue>(
    () => ({
      supported: client.supported,
      status,
      busy,
      refreshStatus,
      fetchProviders,
      signInOAuth,
      signInToken,
      confirmAccountSwitch,
      cancelAccountSwitch,
      syncNow,
      signOut,
      deleteAccount,
    }),
    [
      client.supported,
      status,
      busy,
      refreshStatus,
      fetchProviders,
      signInOAuth,
      signInToken,
      confirmAccountSwitch,
      cancelAccountSwitch,
      syncNow,
      signOut,
      deleteAccount,
    ],
  )

  return <SyncContext.Provider value={value}>{children}</SyncContext.Provider>
}

export function useSync(): SyncContextValue {
  const ctx = useContext(SyncContext)
  if (!ctx) {
    throw new Error('useSync must be used within a SyncProvider')
  }
  return ctx
}

export function useSyncOptional(): SyncContextValue | null {
  return useContext(SyncContext)
}
