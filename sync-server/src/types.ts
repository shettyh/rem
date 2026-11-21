import type { UserSyncDO } from './userSyncDo'

export interface Env {
  USER_SYNC: DurableObjectNamespace<UserSyncDO>
  ASSETS: R2Bucket
  /** Optional dev/self-host bearer token (`Bearer <token>` or `Bearer <token>:<userId>`). */
  DEV_AUTH_TOKEN?: string
  /** Optional HS256 JWT signing secret for access tokens (`sub` = userId). */
  JWT_SECRET?: string
  /** Maximum single asset upload size in bytes (default 10 MB). */
  MAX_ASSET_BYTES?: string
  /** Maximum total stored asset bytes per user (default 500 MB). */
  MAX_USER_QUOTA_BYTES?: string
}

export interface SyncOpEnvelope {
  opId: string
  payload: Record<string, unknown>
}

export interface SyncRequest {
  deviceId: string
  sinceSeq: number
  limit?: number
  ops: SyncOpEnvelope[]
}

export interface RemoteSyncOp {
  seq: number
  opId: string
  deviceId: string
  payload: Record<string, unknown>
}

export interface SyncResponse {
  latestSeq: number
  hasMore: boolean
  ackedOpIds: string[]
  ops: RemoteSyncOp[]
}

export interface MissingAssetsRequest {
  hashes: string[]
}

export interface MissingAssetsResponse {
  missing: string[]
}

export type QuotaCheckResult =
  | { status: 'already_present' }
  | { status: 'allowed' }
  | { status: 'quota_exceeded' }
