import type { UserSyncDO } from './userSyncDo'

export type OAuthProvider = 'google' | 'apple'

export interface Env {
  USER_SYNC: DurableObjectNamespace<UserSyncDO>
  ASSETS: R2Bucket
  /** D1 database for users, OAuth identities, refresh tokens, and PKCE codes. */
  DB?: D1Database
  /** Optional dev/self-host bearer token (`Bearer <token>` or `Bearer <token>:<userId>`). */
  DEV_AUTH_TOKEN?: string
  /** HS256 JWT signing secret for access tokens (`sub` = userId) and OAuth state. */
  JWT_SECRET?: string
  /** Maximum single asset upload size in bytes (default 10 MB). */
  MAX_ASSET_BYTES?: string
  /** Maximum total stored asset bytes per user (default 500 MB). */
  MAX_USER_QUOTA_BYTES?: string
  /** When "true", only subscribed or allowlisted users may sync or access R2 assets (HTTP 402 otherwise). */
  REQUIRE_SUBSCRIPTION?: string
  /** Optional comma-separated emails always permitted to sync when REQUIRE_SUBSCRIPTION="true". */
  ALLOWED_EMAILS?: string
  /** Optional checkout/pricing URL advertised to users when subscription is required. */
  UPGRADE_URL?: string

  /** Google OAuth 2.0 credentials. */
  GOOGLE_CLIENT_ID?: string
  GOOGLE_CLIENT_SECRET?: string
  GOOGLE_AUTH_URL?: string
  GOOGLE_TOKEN_URL?: string
  GOOGLE_JWKS_URL?: string

  /** Sign in with Apple credentials. */
  APPLE_CLIENT_ID?: string
  APPLE_TEAM_ID?: string
  APPLE_KEY_ID?: string
  /** PKCS#8 PEM-encoded ES256 private key (.p8 contents). */
  APPLE_PRIVATE_KEY?: string
  APPLE_AUTH_URL?: string
  APPLE_TOKEN_URL?: string
  APPLE_JWKS_URL?: string
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

export type SubscriptionStatus =
  | 'none'
  | 'active'
  | 'trialing'
  | 'past_due'
  | 'canceled'
  | 'allowlisted'

export interface AuthProvidersResponse {
  google: boolean
  apple: boolean
  devToken: boolean
  subscriptionRequired: boolean
  upgradeUrl: string | null
}

export interface AccountInfo {
  id: string
  email: string | null
  providers: string[]
  authMethod: 'jwt' | 'dev_token'
  syncEnabled: boolean
  subscriptionRequired: boolean
  subscriptionStatus: SubscriptionStatus
  upgradeUrl: string | null
}

export interface TokenExchangeRequest {
  code: string
  codeVerifier: string
  redirectUri: string
  deviceId?: string
}

export interface TokenRefreshRequest {
  refreshToken: string
  deviceId?: string
}

export interface LogoutRequest {
  refreshToken: string
}

export interface AuthTokenResponse {
  accessToken: string
  expiresIn: number
  refreshToken: string
  account: {
    id: string
    email: string | null
    provider: string
  }
}
