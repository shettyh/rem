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
import {
  defaultUpdaterClient,
  type CliStatus,
  type DownloadProgress,
  type PendingUpdateHandle,
  type UpdateInfo,
  type UpdaterClient,
} from './UpdaterClient'
import {
  getAutoUpdateEnabled,
  setAutoUpdateEnabled as persistAutoUpdateEnabled,
} from './updaterPrefs'

const STARTUP_CHECK_DELAY_MS = 4_000
const PERIODIC_CHECK_INTERVAL_MS = 4 * 60 * 60 * 1_000

export type UpdateState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'up-to-date'; checkedAt: number }
  | { status: 'available'; info: UpdateInfo }
  | { status: 'downloading'; info: UpdateInfo; progress: DownloadProgress }
  | { status: 'ready'; info: UpdateInfo }
  | { status: 'error'; message: string }

export interface UpdaterContextValue {
  supported: boolean
  state: UpdateState
  autoUpdateEnabled: boolean
  relaunching: boolean
  cliStatus: CliStatus | null
  cliBusy: boolean
  cliError: string | null
  setAutoUpdateEnabled: (enabled: boolean) => void
  checkForUpdates: (opts?: { silent?: boolean; autoDownload?: boolean }) => Promise<void>
  downloadAndInstall: () => Promise<void>
  relaunch: () => Promise<void>
  refreshCliStatus: () => Promise<void>
  installCli: () => Promise<void>
}

const UpdaterContext = createContext<UpdaterContextValue | null>(null)

function formatUpdateError(err: unknown): string {
  if (err instanceof Error && err.message.trim()) return err.message
  if (typeof err === 'string' && err.trim()) return err
  return 'Could not check for updates right now.'
}

export function UpdaterProvider({
  children,
  client = defaultUpdaterClient,
  autoCheck = true,
  startupDelayMs = STARTUP_CHECK_DELAY_MS,
  pollIntervalMs = PERIODIC_CHECK_INTERVAL_MS,
}: {
  children: ReactNode
  client?: UpdaterClient
  /** Disable automatic background checks in deterministic UI tests. */
  autoCheck?: boolean
  /** Override startup delay before the first automatic update check (for tests). */
  startupDelayMs?: number
  /** Override interval between background update checks (for tests; 0 disables interval). */
  pollIntervalMs?: number
}) {
  const [state, setState] = useState<UpdateState>({ status: 'idle' })
  const [autoUpdateEnabled, setAutoUpdateEnabledState] = useState<boolean>(() =>
    getAutoUpdateEnabled(),
  )
  const [relaunching, setRelaunching] = useState(false)
  const [cliStatus, setCliStatus] = useState<CliStatus | null>(null)
  const [cliBusy, setCliBusy] = useState(false)
  const [cliError, setCliError] = useState<string | null>(null)

  const pendingRef = useRef<PendingUpdateHandle | null>(null)
  const busyRef = useRef(false)
  const relaunchingRef = useRef(false)
  const mountedRef = useRef(true)
  const autoUpdateEnabledRef = useRef(autoUpdateEnabled)
  const stateRef = useRef<UpdateState>(state)

  autoUpdateEnabledRef.current = autoUpdateEnabled

  const updateState = useCallback((next: UpdateState) => {
    stateRef.current = next
    setState(next)
  }, [])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      const pending = pendingRef.current
      pendingRef.current = null
      void pending?.close?.()
    }
  }, [])

  const replacePending = useCallback((next: PendingUpdateHandle | null) => {
    const prev = pendingRef.current
    pendingRef.current = next
    if (prev && prev !== next) {
      void prev.close?.()
    }
  }, [])

  const stagePending = useCallback(
    async (pending: PendingUpdateHandle, { silent }: { silent: boolean }) => {
      updateState({
        status: 'downloading',
        info: pending.info,
        progress: { downloaded: 0, total: null },
      })
      try {
        await pending.downloadAndInstall((progress) => {
          if (!mountedRef.current) return
          updateState({ status: 'downloading', info: pending.info, progress })
        })
        if (!mountedRef.current) return
        updateState({ status: 'ready', info: pending.info })
      } catch (err) {
        if (!mountedRef.current) return
        if (silent) {
          // Keep the update available for manual install instead of surfacing an unsolicited error.
          updateState({ status: 'available', info: pending.info })
        } else {
          updateState({ status: 'error', message: formatUpdateError(err) })
        }
      }
    },
    [updateState],
  )

  const refreshCliStatus = useCallback(async () => {
    if (!client.supported) return
    try {
      const next = await client.getCliStatus()
      if (mountedRef.current) setCliStatus(next)
    } catch {
      // Non-fatal: CLI status is informational.
    }
  }, [client])

  const checkForUpdates = useCallback(
    async ({
      silent = false,
      autoDownload = false,
    }: { silent?: boolean; autoDownload?: boolean } = {}) => {
      if (!client.supported || busyRef.current) return
      if (
        silent &&
        (stateRef.current.status === 'ready' || stateRef.current.status === 'downloading')
      ) {
        return
      }
      busyRef.current = true
      if (!silent) {
        updateState({ status: 'checking' })
      }
      try {
        const pending = await client.check()
        if (!mountedRef.current) {
          void pending?.close?.()
          return
        }
        replacePending(pending)
        if (pending) {
          if (autoDownload && autoUpdateEnabledRef.current) {
            await stagePending(pending, { silent })
          } else {
            updateState({ status: 'available', info: pending.info })
          }
        } else if (!silent) {
          updateState({ status: 'up-to-date', checkedAt: Date.now() })
        }
      } catch (err) {
        if (!silent && mountedRef.current) {
          updateState({ status: 'error', message: formatUpdateError(err) })
        }
      } finally {
        busyRef.current = false
      }
    },
    [client, replacePending, stagePending, updateState],
  )

  const downloadAndInstall = useCallback(async () => {
    const pending = pendingRef.current
    if (!client.supported || !pending || busyRef.current) return
    busyRef.current = true
    try {
      await stagePending(pending, { silent: false })
    } finally {
      busyRef.current = false
    }
  }, [client.supported, stagePending])

  const setAutoUpdateEnabled = useCallback(
    (enabled: boolean) => {
      persistAutoUpdateEnabled(enabled)
      autoUpdateEnabledRef.current = enabled
      setAutoUpdateEnabledState(enabled)

      if (
        enabled &&
        client.supported &&
        stateRef.current.status === 'available' &&
        pendingRef.current &&
        !busyRef.current
      ) {
        const pending = pendingRef.current
        busyRef.current = true
        void stagePending(pending, { silent: true }).finally(() => {
          busyRef.current = false
        })
      }
    },
    [client.supported, stagePending],
  )

  const relaunch = useCallback(async () => {
    if (!client.supported || relaunchingRef.current) return
    relaunchingRef.current = true
    setRelaunching(true)
    try {
      const handledByInstaller = (await pendingRef.current?.finishInstall?.()) ?? false
      if (!handledByInstaller) {
        await client.relaunch()
      }
    } catch (err) {
      if (mountedRef.current) {
        updateState({ status: 'error', message: formatUpdateError(err) })
      }
    } finally {
      if (mountedRef.current) {
        relaunchingRef.current = false
        setRelaunching(false)
      }
    }
  }, [client, updateState])

  const installCli = useCallback(async () => {
    if (!client.supported) return
    setCliBusy(true)
    setCliError(null)
    try {
      const next = await client.installCli()
      if (mountedRef.current) setCliStatus(next)
    } catch (err) {
      if (mountedRef.current) setCliError(formatUpdateError(err))
    } finally {
      if (mountedRef.current) setCliBusy(false)
    }
  }, [client])

  useEffect(() => {
    if (!client.supported) return
    void refreshCliStatus()
  }, [client.supported, refreshCliStatus])

  useEffect(() => {
    if (!client.supported || !autoCheck || !autoUpdateEnabled) return
    const startupTimer = window.setTimeout(() => {
      void checkForUpdates({ silent: true, autoDownload: true })
    }, startupDelayMs)
    const intervalTimer =
      pollIntervalMs > 0
        ? window.setInterval(() => {
            void checkForUpdates({ silent: true, autoDownload: true })
          }, pollIntervalMs)
        : null
    return () => {
      window.clearTimeout(startupTimer)
      if (intervalTimer !== null) window.clearInterval(intervalTimer)
    }
  }, [autoCheck, autoUpdateEnabled, checkForUpdates, client.supported, pollIntervalMs, startupDelayMs])

  const value = useMemo<UpdaterContextValue>(
    () => ({
      supported: client.supported,
      state,
      autoUpdateEnabled,
      relaunching,
      cliStatus,
      cliBusy,
      cliError,
      setAutoUpdateEnabled,
      checkForUpdates,
      downloadAndInstall,
      relaunch,
      refreshCliStatus,
      installCli,
    }),
    [
      client.supported,
      state,
      autoUpdateEnabled,
      relaunching,
      cliStatus,
      cliBusy,
      cliError,
      setAutoUpdateEnabled,
      checkForUpdates,
      downloadAndInstall,
      relaunch,
      refreshCliStatus,
      installCli,
    ],
  )

  return <UpdaterContext.Provider value={value}>{children}</UpdaterContext.Provider>
}

export function useUpdaterOptional(): UpdaterContextValue | null {
  return useContext(UpdaterContext)
}
