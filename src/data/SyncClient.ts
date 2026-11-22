import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export const DEFAULT_CLOUD_SERVER_URL = 'https://rem-sync.shettyh.fyi'

export type SyncMode = 'local' | 'cloud'
export type SyncActivity = 'idle' | 'syncing' | 'error'
export type OAuthProvider = 'google' | 'apple'

export type EntitlementState =
  | 'self_hosted'
  | 'none'
  | 'trialing'
  | 'invite'
  | 'active'
  | 'past_due'
  | 'expired'

export interface EntitlementInfo {
  state: EntitlementState
  syncEnabled: boolean
  provider: string | null
  plan: string | null
  expiresAt: number | null
  cancelAtPeriodEnd: boolean
  canStartTrial: boolean
  trialDays: number
  priceDisplay: string
  canSubscribePolar: boolean
  canManageBilling: boolean
}

export interface SyncStatus {
  mode: SyncMode
  activity: SyncActivity
  defaultServerUrl: string
  serverUrl: string | null
  accountId: string | null
  email: string | null
  provider: string | null
  signedIn: boolean
  deviceId: string
  lastSeq: number
  pendingOutbox: number
  lastSyncedAt: number | null
  lastError: string | null
  subscriptionRequired: boolean
  upgradeUrl: string | null
  entitlement: EntitlementInfo | null
}

export interface AuthProviders {
  google: boolean
  apple: boolean
  devToken: boolean
  subscriptionRequired: boolean
  upgradeUrl: string | null
}

export type SignInOutcome =
  | {
      status: 'connected'
      snapshot: SyncStatus
      synced: boolean
      syncError: string | null
    }
  | {
      status: 'accountMismatch'
      boundServerUrl: string
      boundAccountId: string
      incomingServerUrl: string
      incomingAccountId: string
      incomingEmail: string | null
      incomingProvider: string
    }

export interface SyncClient {
  readonly supported: boolean
  getStatus(): Promise<SyncStatus>
  fetchProviders(serverUrl: string): Promise<AuthProviders>
  signInOAuth(serverUrl: string, provider: OAuthProvider): Promise<SignInOutcome>
  signInToken(serverUrl: string, token: string): Promise<SignInOutcome>
  confirmAccountSwitch(): Promise<SignInOutcome>
  cancelAccountSwitch(): Promise<void>
  syncNow(): Promise<SyncStatus>
  refreshAccount(): Promise<SyncStatus>
  startTrial(): Promise<SyncStatus>
  redeemInvite(code: string): Promise<SyncStatus>
  signOut(): Promise<SyncStatus>
  deleteAccount(): Promise<SyncStatus>
  openExternalUrl(url: string): Promise<void>
  subscribeStatus(listener: (status: SyncStatus) => void): () => void
  subscribeRemoteApplied(listener: (appliedOps: number) => void): () => void
}

export const EVENT_SYNC_STATE_CHANGED = 'sync:state-changed'
export const EVENT_STORAGE_REMOTE_APPLIED = 'storage:remote-applied'

export class TauriSyncClient implements SyncClient {
  readonly supported = true

  getStatus(): Promise<SyncStatus> {
    return invoke<SyncStatus>('sync_get_status')
  }

  fetchProviders(serverUrl: string): Promise<AuthProviders> {
    return invoke<AuthProviders>('sync_fetch_providers', { serverUrl })
  }

  signInOAuth(serverUrl: string, provider: OAuthProvider): Promise<SignInOutcome> {
    return invoke<SignInOutcome>('sync_sign_in_oauth', { serverUrl, provider })
  }

  signInToken(serverUrl: string, token: string): Promise<SignInOutcome> {
    return invoke<SignInOutcome>('sync_sign_in_token', { serverUrl, token })
  }

  confirmAccountSwitch(): Promise<SignInOutcome> {
    return invoke<SignInOutcome>('sync_confirm_account_switch')
  }

  cancelAccountSwitch(): Promise<void> {
    return invoke<void>('sync_cancel_account_switch')
  }

  syncNow(): Promise<SyncStatus> {
    return invoke<SyncStatus>('sync_now')
  }

  refreshAccount(): Promise<SyncStatus> {
    return invoke<SyncStatus>('sync_refresh_account')
  }

  startTrial(): Promise<SyncStatus> {
    return invoke<SyncStatus>('sync_start_trial')
  }

  redeemInvite(code: string): Promise<SyncStatus> {
    return invoke<SyncStatus>('sync_redeem_invite', { code })
  }

  signOut(): Promise<SyncStatus> {
    return invoke<SyncStatus>('sync_sign_out')
  }

  deleteAccount(): Promise<SyncStatus> {
    return invoke<SyncStatus>('sync_delete_account')
  }

  openExternalUrl(url: string): Promise<void> {
    return invoke<void>('sync_open_external_url', { url })
  }

  subscribeStatus(listener: (status: SyncStatus) => void): () => void {
    let active = true
    let unlisten: (() => void) | null = null
    void listen<SyncStatus>(EVENT_SYNC_STATE_CHANGED, (event) => {
      if (active) listener(event.payload)
    }).then((fn) => {
      if (active) unlisten = fn
      else fn()
    })
    return () => {
      active = false
      unlisten?.()
    }
  }

  subscribeRemoteApplied(listener: (appliedOps: number) => void): () => void {
    let active = true
    let unlisten: (() => void) | null = null
    void listen<number>(EVENT_STORAGE_REMOTE_APPLIED, (event) => {
      if (active) listener(event.payload)
    }).then((fn) => {
      if (active) unlisten = fn
      else fn()
    })
    return () => {
      active = false
      unlisten?.()
    }
  }
}

const DEFAULT_LOCAL_STATUS: SyncStatus = {
  mode: 'local',
  activity: 'idle',
  defaultServerUrl: DEFAULT_CLOUD_SERVER_URL,
  serverUrl: null,
  accountId: null,
  email: null,
  provider: null,
  signedIn: false,
  deviceId: 'local-browser',
  lastSeq: 0,
  pendingOutbox: 0,
  lastSyncedAt: null,
  lastError: null,
  subscriptionRequired: false,
  upgradeUrl: null,
  entitlement: null,
}

export class LocalOnlySyncClient implements SyncClient {
  readonly supported = false

  async getStatus(): Promise<SyncStatus> {
    return { ...DEFAULT_LOCAL_STATUS }
  }

  async fetchProviders(): Promise<AuthProviders> {
    throw new Error('Cloud sync is only available in the desktop app.')
  }

  async signInOAuth(): Promise<SignInOutcome> {
    throw new Error('Cloud sync is only available in the desktop app.')
  }

  async signInToken(): Promise<SignInOutcome> {
    throw new Error('Cloud sync is only available in the desktop app.')
  }

  async confirmAccountSwitch(): Promise<SignInOutcome> {
    throw new Error('Cloud sync is only available in the desktop app.')
  }

  async cancelAccountSwitch(): Promise<void> {}

  async syncNow(): Promise<SyncStatus> {
    return { ...DEFAULT_LOCAL_STATUS }
  }

  async refreshAccount(): Promise<SyncStatus> {
    return { ...DEFAULT_LOCAL_STATUS }
  }

  async startTrial(): Promise<SyncStatus> {
    throw new Error('Cloud sync is only available in the desktop app.')
  }

  async redeemInvite(): Promise<SyncStatus> {
    throw new Error('Cloud sync is only available in the desktop app.')
  }

  async signOut(): Promise<SyncStatus> {
    return { ...DEFAULT_LOCAL_STATUS }
  }

  async deleteAccount(): Promise<SyncStatus> {
    return { ...DEFAULT_LOCAL_STATUS }
  }

  async openExternalUrl(url: string): Promise<void> {
    window.open(url, '_blank', 'noopener,noreferrer')
  }

  subscribeStatus(): () => void {
    return () => {}
  }

  subscribeRemoteApplied(): () => void {
    return () => {}
  }
}

export const defaultSyncClient: SyncClient = isTauri()
  ? new TauriSyncClient()
  : new LocalOnlySyncClient()
