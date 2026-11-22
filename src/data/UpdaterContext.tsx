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

const STARTUP_CHECK_DELAY_MS = 4_000

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
  cliStatus: CliStatus | null
  cliBusy: boolean
  cliError: string | null
  checkForUpdates: (opts?: { silent?: boolean }) => Promise<void>
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
}: {
  children: ReactNode
  client?: UpdaterClient
  /** Disable automatic background checks in deterministic UI tests. */
  autoCheck?: boolean
}) {
  const [state, setState] = useState<UpdateState>({ status: 'idle' })
  const [cliStatus, setCliStatus] = useState<CliStatus | null>(null)
  const [cliBusy, setCliBusy] = useState(false)
  const [cliError, setCliError] = useState<string | null>(null)
  const pendingRef = useRef<PendingUpdateHandle | null>(null)
  const busyRef = useRef(false)

  const refreshCliStatus = useCallback(async () => {
    if (!client.supported) return
    try {
      const next = await client.getCliStatus()
      setCliStatus(next)
    } catch {
      // Non-fatal: CLI status is informational.
    }
  }, [client])

  const checkForUpdates = useCallback(
    async ({ silent = false }: { silent?: boolean } = {}) => {
      if (!client.supported || busyRef.current) return
      busyRef.current = true
      if (!silent) {
        setState({ status: 'checking' })
      }
      try {
        const pending = await client.check()
        pendingRef.current = pending
        if (pending) {
          setState({ status: 'available', info: pending.info })
        } else if (!silent) {
          setState({ status: 'up-to-date', checkedAt: Date.now() })
        }
      } catch (err) {
        if (!silent) {
          setState({ status: 'error', message: formatUpdateError(err) })
        }
      } finally {
        busyRef.current = false
      }
    },
    [client],
  )

  const downloadAndInstall = useCallback(async () => {
    const pending = pendingRef.current
    if (!client.supported || !pending || busyRef.current) return
    busyRef.current = true
    setState({
      status: 'downloading',
      info: pending.info,
      progress: { downloaded: 0, total: null },
    })
    try {
      await pending.downloadAndInstall((progress) => {
        setState({ status: 'downloading', info: pending.info, progress })
      })
      setState({ status: 'ready', info: pending.info })
    } catch (err) {
      setState({ status: 'error', message: formatUpdateError(err) })
    } finally {
      busyRef.current = false
    }
  }, [client])

  const relaunch = useCallback(async () => {
    if (!client.supported) return
    await client.relaunch()
  }, [client])

  const installCli = useCallback(async () => {
    if (!client.supported) return
    setCliBusy(true)
    setCliError(null)
    try {
      const next = await client.installCli()
      setCliStatus(next)
    } catch (err) {
      setCliError(formatUpdateError(err))
    } finally {
      setCliBusy(false)
    }
  }, [client])

  useEffect(() => {
    if (!client.supported) return
    void refreshCliStatus()
    if (!autoCheck) return
    const timer = window.setTimeout(() => {
      void checkForUpdates({ silent: true })
    }, STARTUP_CHECK_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [autoCheck, checkForUpdates, client.supported, refreshCliStatus])

  const value = useMemo<UpdaterContextValue>(
    () => ({
      supported: client.supported,
      state,
      cliStatus,
      cliBusy,
      cliError,
      checkForUpdates,
      downloadAndInstall,
      relaunch,
      refreshCliStatus,
      installCli,
    }),
    [
      client.supported,
      state,
      cliStatus,
      cliBusy,
      cliError,
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
