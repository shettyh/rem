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
  /** Trigger background compaction once uncompacted ops reach this count (default 5000). */
  COMPACT_AFTER_OPS?: string
  /** Minimum trailing ops retained in DO SQLite when op pruning is enabled (default 2000). */
  RETAIN_TAIL_OPS?: string
  /** Minimum age in days before compacted ops are eligible for pruning (default 30). */
  RETAIN_TAIL_DAYS?: string
  /** When "true", prune compacted ops outside the retention window (enabled in wrangler.toml; unset behaves as "false"). */
  ENABLE_OP_PRUNING?: string
  /** Grace period in days before unreferenced R2 assets are garbage collected (default 30). */
  ASSET_GC_GRACE_DAYS?: string
  /** When "true", only subscribed, trialing, or allowlisted users may sync or access R2 assets (HTTP 402 otherwise). */
  REQUIRE_SUBSCRIPTION?: string
  /** Optional comma-separated emails always permitted to sync when REQUIRE_SUBSCRIPTION="true". */
  ALLOWED_EMAILS?: string
  /** Optional checkout/pricing URL advertised to users when subscription is required. */
  UPGRADE_URL?: string
  /** Optional length of the no-card free trial in days (default "30"). */
  TRIAL_DAYS?: string
  /** Optional human-readable subscription price display (default "$5/year"). */
  CLOUD_PRICE_DISPLAY?: string

  /** Polar billing credentials (Slice 2). */
  POLAR_ACCESS_TOKEN?: string
  POLAR_PRODUCT_ID?: string
  POLAR_WEBHOOK_SECRET?: string
  POLAR_API_BASE?: string

  /** Apple StoreKit 2 / App Store Server Notifications v2 configuration (Slice 3). */
  APPLE_BUNDLE_ID?: string
  APPLE_PRODUCT_ID?: string
  /** Allowed App Store environments: "Production", "Sandbox", or comma-separated "Production,Sandbox". */
  APPLE_ENVIRONMENT?: string
  /** Optional base64 DER root CA override for deterministic JWS unit tests (defaults strictly to pinned Apple Root CA - G3). */
  APPLE_ROOT_CA_DER_B64?: string

  /** Google OAuth 2.0 credentials. */
  GOOGLE_CLIENT_ID?: string
  GOOGLE_CLIENT_SECRET?: string
  GOOGLE_AUTH_URL?: string
  GOOGLE_TOKEN_URL?: string
  GOOGLE_JWKS_URL?: string

  /** Sign in with Apple credentials. */
  APPLE_CLIENT_ID?: string
  /** Optional native iOS Sign in with Apple bundle ID (defaults to APPLE_BUNDLE_ID, then APPLE_CLIENT_ID). */
  APPLE_IOS_CLIENT_ID?: string
  APPLE_TEAM_ID?: string
  APPLE_KEY_ID?: string
  /** PKCS#8 PEM-encoded ES256 private key (.p8 contents). */
  APPLE_PRIVATE_KEY?: string
  APPLE_AUTH_URL?: string
  APPLE_TOKEN_URL?: string
  APPLE_JWKS_URL?: string
  APPLE_REVOKE_URL?: string
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
  bootstrapRequired?: boolean
  snapshotSeq?: number
}

export interface BootstrapInfo {
  snapshotSeq: number
  snapshotR2Key: string
}

export interface PokeMessage {
  type: 'poke'
  latestSeq: number
}

export type SyncRpcResult =
  | { status: 'ok'; response: SyncResponse }
  | { status: 'invalid'; error: string }

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

export type EntitlementState =
  | 'self_hosted'
  | 'none'
  | 'trialing'
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
  entitlement: EntitlementInfo
}

export interface TokenExchangeRequest {
  code: string
  codeVerifier: string
  redirectUri: string
  deviceId?: string
}

export interface AppleNativeAuthRequest {
  identityToken: string
  authorizationCode?: string
  nonce: string
  email?: string | null
  deviceId?: string
}

export interface TokenRefreshRequest {
  refreshToken: string
  deviceId?: string
}

export interface LogoutRequest {
  refreshToken: string
}

export interface PolarCheckoutResponse {
  checkoutUrl: string
}

export interface PolarPortalResponse {
  portalUrl: string
}

export interface AppleVerifyRequest {
  signedTransactionInfo: string
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
