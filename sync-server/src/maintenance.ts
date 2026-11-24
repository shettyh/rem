import type { Env } from './types'

export const INVITE_RATE_LIMIT_WINDOW_MS = 15 * 60_000
export const INVITE_USER_MAX_ATTEMPTS = 10
export const INVITE_IP_MAX_ATTEMPTS = 20

/** Keep revoked refresh tokens for 7 days so token-family reuse detection catches recent replays. */
export const REVOKED_REFRESH_TOKEN_RETENTION_MS = 7 * 86_400_000
export const RATE_LIMIT_RETENTION_MS = 60 * 60_000

export interface RateLimitResult {
  allowed: boolean
  retryAfterSec: number
}

export interface CleanupReport {
  oauthCodes: number
  refreshTokens: number
  rateLimits: number
}

export function extractClientIp(request: Request): string | null {
  const cfIp = request.headers.get('CF-Connecting-IP')?.trim()
  if (cfIp) return cfIp.slice(0, 64)
  const forwarded = request.headers.get('X-Forwarded-For')
  if (forwarded) {
    const first = forwarded.split(',')[0]?.trim()
    if (first) return first.slice(0, 64)
  }
  return null
}

async function incrementBucket(
  db: D1Database,
  scope: string,
  bucketKey: string,
  windowStart: number,
): Promise<number> {
  const row = await db
    .prepare(
      `INSERT INTO rate_limits(scope, bucket_key, window_start, attempts)
       VALUES (?1, ?2, ?3, 1)
       ON CONFLICT(scope, bucket_key, window_start) DO UPDATE SET
         attempts = attempts + 1
       RETURNING attempts`,
    )
    .bind(scope, bucketKey, windowStart)
    .first<{ attempts: number }>()
  return row?.attempts ?? 1
}

/**
 * Lightweight fixed-window throttle for invite redemption backed by D1.
 * Degrades cleanly if migration 0006 has not been applied yet.
 */
export async function checkInviteRateLimit(
  db: D1Database | undefined,
  userId: string,
  clientIp: string | null,
  now = Date.now(),
): Promise<RateLimitResult> {
  if (!db) return { allowed: true, retryAfterSec: 0 }

  const windowStart = Math.floor(now / INVITE_RATE_LIMIT_WINDOW_MS) * INVITE_RATE_LIMIT_WINDOW_MS
  const retryAfterSec = Math.max(
    1,
    Math.ceil((windowStart + INVITE_RATE_LIMIT_WINDOW_MS - now) / 1000),
  )

  try {
    const userAttempts = await incrementBucket(db, 'invite_user', userId, windowStart)
    if (userAttempts > INVITE_USER_MAX_ATTEMPTS) {
      return { allowed: false, retryAfterSec }
    }
    if (clientIp) {
      const ipAttempts = await incrementBucket(db, 'invite_ip', clientIp, windowStart)
      if (ipAttempts > INVITE_IP_MAX_ATTEMPTS) {
        return { allowed: false, retryAfterSec }
      }
    }
    return { allowed: true, retryAfterSec: 0 }
  } catch {
    // Allow redemption to proceed if migration 0006 has not been applied yet.
    return { allowed: true, retryAfterSec: 0 }
  }
}

/**
 * Purge expired PKCE codes, expired or long-revoked refresh tokens, and stale rate-limit buckets.
 * Invoked daily by the Cloudflare Cron Trigger (`scheduled`).
 */
export async function cleanupExpiredRecords(
  env: Env,
  now = Date.now(),
): Promise<CleanupReport> {
  if (!env.DB) {
    return { oauthCodes: 0, refreshTokens: 0, rateLimits: 0 }
  }

  let oauthCodes = 0
  let refreshTokens = 0
  let rateLimits = 0

  try {
    const res = await env.DB.prepare('DELETE FROM oauth_codes WHERE expires_at <= ?1')
      .bind(now)
      .run()
    oauthCodes = res.meta.changes ?? 0
  } catch {
    // Ignore if oauth_codes table is not migrated yet.
  }

  try {
    const revokedCutoff = now - REVOKED_REFRESH_TOKEN_RETENTION_MS
    const res = await env.DB.prepare(
      `DELETE FROM refresh_tokens
       WHERE expires_at <= ?1
          OR (revoked_at IS NOT NULL AND revoked_at <= ?2)`,
    )
      .bind(now, revokedCutoff)
      .run()
    refreshTokens = res.meta.changes ?? 0
  } catch {
    // Ignore if refresh_tokens table is not migrated yet.
  }

  try {
    const bucketCutoff = now - RATE_LIMIT_RETENTION_MS
    const res = await env.DB.prepare('DELETE FROM rate_limits WHERE window_start <= ?1')
      .bind(bucketCutoff)
      .run()
    rateLimits = res.meta.changes ?? 0
  } catch {
    // Ignore if migration 0006 has not been applied yet.
  }

  return { oauthCodes, refreshTokens, rateLimits }
}
