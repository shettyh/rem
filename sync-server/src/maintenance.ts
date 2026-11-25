import type { Env } from './types'

/** Keep revoked refresh tokens for 7 days so token-family reuse detection catches recent replays. */
export const REVOKED_REFRESH_TOKEN_RETENTION_MS = 7 * 86_400_000

export interface CleanupReport {
  oauthCodes: number
  refreshTokens: number
}

/**
 * Purge expired PKCE codes and expired or long-revoked refresh tokens.
 * Invoked daily by the Cloudflare Cron Trigger (`scheduled`).
 */
export async function cleanupExpiredRecords(
  env: Env,
  now = Date.now(),
): Promise<CleanupReport> {
  if (!env.DB) {
    return { oauthCodes: 0, refreshTokens: 0 }
  }

  let oauthCodes = 0
  let refreshTokens = 0

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

  return { oauthCodes, refreshTokens }
}
