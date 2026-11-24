import { env } from 'cloudflare:test'
import { beforeAll, describe, expect, it } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import rateLimitsMigrationSql from '../migrations/0006_rate_limits_and_cleanup.sql?raw'
import { signHs256Jwt } from '../src/auth'
import worker from '../src/index'
import { INVITE_USER_MAX_ATTEMPTS } from '../src/maintenance'
import { isAllowedClientRedirectUri } from '../src/oauth'
import type { AccountInfo, Env } from '../src/types'

const TEST_ENV = env as unknown as Env

async function applyMigrations() {
  for (const raw of [
    migrationSql,
    entitlementsMigrationSql,
    earlyAccessMigrationSql,
    trialInvitesBillingMigrationSql,
    rateLimitsMigrationSql,
  ]) {
    const statements = raw
      .split(';')
      .map((s: string) => s.trim())
      .filter((s: string) => s.length > 0)
    for (const sql of statements) {
      await TEST_ENV.DB!.prepare(sql).run()
    }
  }
}

async function createUser(userId: string, email: string): Promise<string> {
  const now = Date.now()
  await TEST_ENV.DB!.prepare(
    'INSERT OR IGNORE INTO users(id, email, created_at) VALUES (?1, ?2, ?3)',
  )
    .bind(userId, email, now)
    .run()
  await TEST_ENV.DB!.prepare(
    `INSERT OR IGNORE INTO auth_identities(provider, provider_sub, user_id, email, created_at)
     VALUES ('google', ?1, ?2, ?3, ?4)`,
  )
    .bind(`sub_${userId}`, userId, email, now)
    .run()

  const nowSec = Math.floor(now / 1000)
  return signHs256Jwt(
    { sub: userId, typ: 'access', iat: nowSec, exp: nowSec + 900 },
    TEST_ENV.JWT_SECRET!,
  )
}

function callWorker(url: string, init?: RequestInit, customEnv: Env = TEST_ENV): Promise<Response> {
  return worker.fetch(new Request(url, init), customEnv)
}

const GATED_ENV: Env = {
  ...TEST_ENV,
  REQUIRE_SUBSCRIPTION: 'true',
  TRIAL_DAYS: '30',
  CLOUD_PRICE_DISPLAY: '$5/year',
}

describe('Unified entitlements, optional 30-day trial, and invite codes', () => {
  beforeAll(async () => {
    await applyMigrations()
  })

  it('transitions none -> trialing -> expired and enforces one trial per account', async () => {
    const token = await createUser('usr_trial_1', 'trial1@example.com')

    // Newly signed-in user starts in `none` without burning the trial.
    const initialResp = await callWorker(
      'https://sync.example.com/v1/account',
      { headers: { Authorization: `Bearer ${token}` } },
      GATED_ENV,
    )
    expect(initialResp.status).toBe(200)
    const initial = (await initialResp.json()) as AccountInfo
    expect(initial.syncEnabled).toBe(false)
    expect(initial.entitlement).toMatchObject({
      state: 'none',
      syncEnabled: false,
      provider: null,
      plan: null,
      expiresAt: null,
      canStartTrial: true,
      trialDays: 30,
      priceDisplay: '$5/year',
      canSubscribePolar: false,
      canManageBilling: false,
    })

    // Sync returns 402 with structured `state: 'none'`.
    const blockedSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'd1', sinceSeq: 0, ops: [] }),
      },
      GATED_ENV,
    )
    expect(blockedSync.status).toBe(402)
    expect(await blockedSync.json()).toMatchObject({
      error: 'subscription_required',
      state: 'none',
    })

    // Opt-in trial start activates sync for 30 days.
    const startResp = await callWorker(
      'https://sync.example.com/v1/trial/start',
      { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      GATED_ENV,
    )
    expect(startResp.status).toBe(200)
    const started = (await startResp.json()) as AccountInfo
    expect(started.syncEnabled).toBe(true)
    expect(started.entitlement).toMatchObject({
      state: 'trialing',
      syncEnabled: true,
      provider: 'trial',
      plan: 'trial_30d',
      canStartTrial: false,
    })
    expect(started.entitlement.expiresAt).toBeGreaterThan(Date.now() + 29 * 86_400_000)

    // Calling POST /v1/trial/start again while trialing is idempotent.
    const repeatResp = await callWorker(
      'https://sync.example.com/v1/trial/start',
      { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      GATED_ENV,
    )
    expect(repeatResp.status).toBe(200)
    const repeated = (await repeatResp.json()) as AccountInfo
    expect(repeated.entitlement.expiresAt).toBe(started.entitlement.expiresAt)

    // Sync now succeeds.
    const okSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'd1', sinceSeq: 0, ops: [] }),
      },
      GATED_ENV,
    )
    expect(okSync.status).toBe(200)

    // Expire the trial in D1.
    await TEST_ENV.DB!.prepare(
      'UPDATE user_entitlements SET current_period_end = ?1 WHERE user_id = ?2',
    )
      .bind(Date.now() - 1000, 'usr_trial_1')
      .run()

    const expiredResp = await callWorker(
      'https://sync.example.com/v1/account',
      { headers: { Authorization: `Bearer ${token}` } },
      GATED_ENV,
    )
    const expired = (await expiredResp.json()) as AccountInfo
    expect(expired.syncEnabled).toBe(false)
    expect(expired.entitlement).toMatchObject({
      state: 'expired',
      syncEnabled: false,
      canStartTrial: false,
    })

    // Sync returns 402 with `state: 'expired'`.
    const expiredSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'd1', sinceSeq: 0, ops: [] }),
      },
      GATED_ENV,
    )
    expect(expiredSync.status).toBe(402)
    expect(await expiredSync.json()).toMatchObject({
      error: 'subscription_required',
      state: 'expired',
    })

    // Cannot start a second trial once trial_started_at is recorded.
    const secondTrial = await callWorker(
      'https://sync.example.com/v1/trial/start',
      { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      GATED_ENV,
    )
    expect(secondTrial.status).toBe(409)
    expect(await secondTrial.json()).toMatchObject({ error: 'trial_already_used' })
  })

  it('redeems single-use and multi-use invite codes atomically, handles errors, and preserves trial_started_at', async () => {
    const now = Date.now()
    const u1 = await createUser('usr_inv_1', 'inv1@example.com')
    const u2 = await createUser('usr_inv_2', 'inv2@example.com')
    const u3 = await createUser('usr_inv_3', 'inv3@example.com')

    await TEST_ENV.DB!.batch([
      TEST_ENV.DB!.prepare(
        `INSERT INTO invite_codes(code, max_uses, uses_count, grant_days, plan_label, note, expires_at, revoked_at, created_at)
         VALUES ('REM-SINGLE-0001', 1, 0, NULL, 'founder_free', 'forever', NULL, NULL, ?1)`,
      ).bind(now),
      TEST_ENV.DB!.prepare(
        `INSERT INTO invite_codes(code, max_uses, uses_count, grant_days, plan_label, note, expires_at, revoked_at, created_at)
         VALUES ('REM-MULTI-0002', 2, 0, 90, 'beta_90d', '90 days', NULL, NULL, ?1)`,
      ).bind(now),
      TEST_ENV.DB!.prepare(
        `INSERT INTO invite_codes(code, max_uses, uses_count, grant_days, plan_label, note, expires_at, revoked_at, created_at)
         VALUES ('REM-REVOKED-003', 5, 0, NULL, 'invite_free', NULL, NULL, ?1, ?1)`,
      ).bind(now),
      TEST_ENV.DB!.prepare(
        `INSERT INTO invite_codes(code, max_uses, uses_count, grant_days, plan_label, note, expires_at, revoked_at, created_at)
         VALUES ('REM-EXPIRED-004', 5, 0, NULL, 'invite_free', NULL, ?1, NULL, ?2)`,
      ).bind(now - 5000, now - 10000),
    ])

    // Invalid / missing / revoked / expired codes rejected cleanly.
    const badFormat = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u1}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: '  ' }),
      },
      GATED_ENV,
    )
    expect(badFormat.status).toBe(400)

    const notFound = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u1}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: 'REM-NOPE-9999' }),
      },
      GATED_ENV,
    )
    expect(notFound.status).toBe(404)

    const revoked = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u1}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: 'REM-REVOKED-003' }),
      },
      GATED_ENV,
    )
    expect(revoked.status).toBe(409)
    expect(await revoked.json()).toMatchObject({ error: 'invite_revoked' })

    const expiredCode = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u1}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: 'REM-EXPIRED-004' }),
      },
      GATED_ENV,
    )
    expect(expiredCode.status).toBe(409)
    expect(await expiredCode.json()).toMatchObject({ error: 'invite_expired' })

    // User 1 starts a trial first, then redeems a time-boxed 90d multi-use invite (case/whitespace normalized).
    await callWorker(
      'https://sync.example.com/v1/trial/start',
      { method: 'POST', headers: { Authorization: `Bearer ${u1}` } },
      GATED_ENV,
    )

    const redeem1 = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u1}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: '  rem-multi-0002 ' }),
      },
      GATED_ENV,
    )
    expect(redeem1.status).toBe(200)
    const acc1 = (await redeem1.json()) as AccountInfo
    expect(acc1.entitlement).toMatchObject({
      state: 'invite',
      syncEnabled: true,
      provider: 'invite',
      plan: 'beta_90d',
      canStartTrial: false,
    })
    expect(acc1.entitlement.expiresAt).toBeGreaterThan(now + 89 * 86_400_000)

    // Verify trial_started_at was preserved on usr_inv_1!
    const entRow1 = await TEST_ENV.DB!.prepare(
      'SELECT trial_started_at FROM user_entitlements WHERE user_id = ?1',
    )
      .bind('usr_inv_1')
      .first<{ trial_started_at: number | null }>()
    expect(entRow1?.trial_started_at).not.toBeNull()

    // Same user cannot redeem the same code twice.
    const dupRedeem = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u1}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: 'REM-MULTI-0002' }),
      },
      GATED_ENV,
    )
    expect(dupRedeem.status).toBe(409)
    expect(await dupRedeem.json()).toMatchObject({ error: 'invite_already_redeemed' })

    // User 2 claims the 2nd use of REM-MULTI-0002; User 3 gets invite_exhausted.
    const redeem2 = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u2}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: 'REM-MULTI-0002' }),
      },
      GATED_ENV,
    )
    expect(redeem2.status).toBe(200)

    const redeem3Exhausted = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u3}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: 'REM-MULTI-0002' }),
      },
      GATED_ENV,
    )
    expect(redeem3Exhausted.status).toBe(409)
    expect(await redeem3Exhausted.json()).toMatchObject({ error: 'invite_exhausted' })

    // User 3 redeems the indefinite single-use code.
    const redeemForever = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${u3}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ code: 'REM-SINGLE-0001' }),
      },
      GATED_ENV,
    )
    expect(redeemForever.status).toBe(200)
    const acc3 = (await redeemForever.json()) as AccountInfo
    expect(acc3.entitlement).toMatchObject({
      state: 'invite',
      syncEnabled: true,
      provider: 'invite',
      plan: 'founder_free',
      expiresAt: null,
    })

    // Deleting User 3 cleans up both invite_redemptions and user_entitlements.
    const delResp = await callWorker(
      'https://sync.example.com/v1/account',
      { method: 'DELETE', headers: { Authorization: `Bearer ${u3}` } },
      GATED_ENV,
    )
    expect(delResp.status).toBe(200)
    const remainingRedemption = await TEST_ENV.DB!.prepare(
      'SELECT code FROM invite_redemptions WHERE user_id = ?1',
    )
      .bind('usr_inv_3')
      .first()
    expect(remainingRedemption).toBeNull()
    const remainingEntitlement = await TEST_ENV.DB!.prepare(
      'SELECT user_id FROM user_entitlements WHERE user_id = ?1',
    )
      .bind('usr_inv_3')
      .first()
    expect(remainingEntitlement).toBeNull()
  })

  it('computes paid active/past_due flags and double-subscription guards accurately', async () => {
    const polarEnv: Env = {
      ...GATED_ENV,
      POLAR_ACCESS_TOKEN: 'polar_test_token',
      POLAR_PRODUCT_ID: 'prod_test_123',
    }
    const now = Date.now()

    const appleToken = await createUser('usr_apple_sub', 'apple-sub@example.com')
    await TEST_ENV.DB!.prepare(
      `INSERT INTO user_entitlements(
         user_id, status, plan, provider, provider_subscription_id, current_period_end,
         cancel_at_period_end, original_transaction_id, updated_at
       ) VALUES ('usr_apple_sub', 'active', 'premium_yearly', 'apple', 'orig_tx_1', ?1, 0, 'orig_tx_1', ?2)`,
    )
      .bind(now + 30 * 86_400_000, now)
      .run()

    const appleAcc = (await (
      await callWorker(
        'https://sync.example.com/v1/account',
        { headers: { Authorization: `Bearer ${appleToken}` } },
        polarEnv,
      )
    ).json()) as AccountInfo
    expect(appleAcc.entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      provider: 'apple',
      canSubscribePolar: false,
      canManageBilling: false,
    })

    const polarToken = await createUser('usr_polar_sub', 'polar-sub@example.com')
    await TEST_ENV.DB!.prepare(
      `INSERT INTO user_entitlements(
         user_id, status, plan, provider, provider_customer_id, provider_subscription_id,
         current_period_end, cancel_at_period_end, updated_at
       ) VALUES ('usr_polar_sub', 'past_due', 'premium_yearly', 'polar', 'cus_123', 'sub_123', ?1, 1, ?2)`,
    )
      .bind(now + 3 * 86_400_000, now)
      .run()

    const polarPastDue = (await (
      await callWorker(
        'https://sync.example.com/v1/account',
        { headers: { Authorization: `Bearer ${polarToken}` } },
        polarEnv,
      )
    ).json()) as AccountInfo
    expect(polarPastDue.entitlement).toMatchObject({
      state: 'past_due',
      syncEnabled: true,
      provider: 'polar',
      cancelAtPeriodEnd: true,
      canManageBilling: true,
    })

    // Once grace window passes, past_due pauses sync cleanly.
    await TEST_ENV.DB!.prepare(
      'UPDATE user_entitlements SET current_period_end = ?1 WHERE user_id = ?2',
    )
      .bind(now - 1000, 'usr_polar_sub')
      .run()

    const polarPastDueLapsed = (await (
      await callWorker(
        'https://sync.example.com/v1/account',
        { headers: { Authorization: `Bearer ${polarToken}` } },
        polarEnv,
      )
    ).json()) as AccountInfo
    expect(polarPastDueLapsed.entitlement).toMatchObject({
      state: 'past_due',
      syncEnabled: false,
      provider: 'polar',
      canManageBilling: true,
    })
  })

  it('allows same-origin /premium and /cloud web PKCE redirect URIs while rejecting cross-origin URIs', () => {
    expect(
      isAllowedClientRedirectUri('https://rem-sync.shettyh.fyi/premium', 'https://rem-sync.shettyh.fyi'),
    ).toBe(true)
    expect(
      isAllowedClientRedirectUri('https://rem-sync.shettyh.fyi/cloud', 'https://rem-sync.shettyh.fyi'),
    ).toBe(true)
    expect(
      isAllowedClientRedirectUri('https://evil.example.com/premium', 'https://rem-sync.shettyh.fyi'),
    ).toBe(false)
    expect(
      isAllowedClientRedirectUri('https://rem-sync.shettyh.fyi/other', 'https://rem-sync.shettyh.fyi'),
    ).toBe(false)
    expect(
      isAllowedClientRedirectUri('https://rem-sync.shettyh.fyi/premium?x=1', 'https://rem-sync.shettyh.fyi'),
    ).toBe(false)
    expect(isAllowedClientRedirectUri('rem://auth/callback', 'https://rem-sync.shettyh.fyi')).toBe(true)
    expect(
      isAllowedClientRedirectUri('http://127.0.0.1:41234/auth/callback', 'https://rem-sync.shettyh.fyi'),
    ).toBe(true)
  })

  it('rate-limits repeated invite redemption probes per user and returns 429 with Retry-After', async () => {
    const token = await createUser('usr_invite_rl_1', 'rl1@example.com')
    const otherToken = await createUser('usr_invite_rl_2', 'rl2@example.com')

    for (let i = 0; i < INVITE_USER_MAX_ATTEMPTS; i++) {
      const resp = await callWorker(
        'https://sync.example.com/v1/invites/redeem',
        {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${token}`,
            'Content-Type': 'application/json',
            'CF-Connecting-IP': '203.0.113.10',
          },
          body: JSON.stringify({ code: `REM-MISS-000${i}` }),
        },
        GATED_ENV,
      )
      expect(resp.status).toBe(404)
    }

    const blocked = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': 'application/json',
          'CF-Connecting-IP': '203.0.113.10',
        },
        body: JSON.stringify({ code: 'REM-MISS-OVER' }),
      },
      GATED_ENV,
    )
    expect(blocked.status).toBe(429)
    expect(Number(blocked.headers.get('Retry-After'))).toBeGreaterThan(0)
    expect(await blocked.json()).toMatchObject({ error: 'rate_limited' })

    // A different user from a different IP is unaffected.
    const unaffected = await callWorker(
      'https://sync.example.com/v1/invites/redeem',
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${otherToken}`,
          'Content-Type': 'application/json',
          'CF-Connecting-IP': '203.0.113.11',
        },
        body: JSON.stringify({ code: 'REM-MISS-OTHER' }),
      },
      GATED_ENV,
    )
    expect(unaffected.status).toBe(404)
  })
})
