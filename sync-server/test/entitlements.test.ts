import { env } from 'cloudflare:test'
import { beforeAll, describe, expect, it } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import rateLimitsMigrationSql from '../migrations/0006_rate_limits_and_cleanup.sql?raw'
import dropInvitesMigrationSql from '../migrations/0007_drop_invites.sql?raw'
import { signHs256Jwt } from '../src/auth'
import worker from '../src/index'
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
    dropInvitesMigrationSql,
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

describe('Unified entitlements and optional 30-day trial', () => {
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

  it('grants active access to ALLOWED_EMAILS and deletes user_entitlements on account deletion', async () => {
    const token = await createUser('usr_vip_1', 'vip@example.com')
    const allowlistEnv: Env = {
      ...GATED_ENV,
      ALLOWED_EMAILS: 'vip@example.com',
    }

    const resp = await callWorker(
      'https://sync.example.com/v1/account',
      { headers: { Authorization: `Bearer ${token}` } },
      allowlistEnv,
    )
    expect(resp.status).toBe(200)
    const acc = (await resp.json()) as AccountInfo
    expect(acc.entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      provider: 'allowlist',
      plan: 'allowlist',
      expiresAt: null,
      canStartTrial: false,
      canSubscribePolar: false,
      canManageBilling: false,
    })

    // Deleting a user with a trial entitlement cleans up user_entitlements.
    const delToken = await createUser('usr_del_ent', 'delent@example.com')
    await callWorker(
      'https://sync.example.com/v1/trial/start',
      { method: 'POST', headers: { Authorization: `Bearer ${delToken}` } },
      GATED_ENV,
    )
    const delResp = await callWorker(
      'https://sync.example.com/v1/account',
      { method: 'DELETE', headers: { Authorization: `Bearer ${delToken}` } },
      GATED_ENV,
    )
    expect(delResp.status).toBe(200)
    const remainingEntitlement = await TEST_ENV.DB!.prepare(
      'SELECT user_id FROM user_entitlements WHERE user_id = ?1',
    )
      .bind('usr_del_ent')
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
})
