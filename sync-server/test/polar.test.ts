import { env } from 'cloudflare:test'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import { signHs256Jwt } from '../src/auth'
import worker from '../src/index'
import { signPolarWebhook } from '../src/polar'
import type { AccountInfo, Env } from '../src/types'

const TEST_ENV = env as unknown as Env
const WEBHOOK_SECRET = 'polar_whs_test_secret_1234567890'
const POLAR_API_BASE = 'https://sandbox-api.polar.sh'

const POLAR_ENV: Env = {
  ...TEST_ENV,
  REQUIRE_SUBSCRIPTION: 'true',
  TRIAL_DAYS: '30',
  CLOUD_PRICE_DISPLAY: '$5/year',
  POLAR_ACCESS_TOKEN: 'polar_oat_test_token',
  POLAR_PRODUCT_ID: 'prod_rem_premium_yearly',
  POLAR_WEBHOOK_SECRET: WEBHOOK_SECRET,
  POLAR_API_BASE,
}

async function applyMigrations() {
  for (const raw of [
    migrationSql,
    entitlementsMigrationSql,
    earlyAccessMigrationSql,
    trialInvitesBillingMigrationSql,
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

function callWorker(url: string, init?: RequestInit, customEnv: Env = POLAR_ENV): Promise<Response> {
  return worker.fetch(new Request(url, init), customEnv)
}

async function sendSignedPolarWebhook(
  payload: Record<string, unknown>,
  opts: {
    webhookId?: string
    timestampSec?: number
    secret?: string
    signatureHeaderOverride?: string
    customEnv?: Env
  } = {},
): Promise<Response> {
  const rawBody = JSON.stringify(payload)
  const webhookId = opts.webhookId ?? `wh_${crypto.randomUUID()}`
  const timestampSec = opts.timestampSec ?? Math.floor(Date.now() / 1000)
  const timestampStr = String(timestampSec)
  const signature =
    opts.signatureHeaderOverride ??
    (await signPolarWebhook(opts.secret ?? WEBHOOK_SECRET, webhookId, timestampStr, rawBody))

  return callWorker(
    'https://sync.example.com/v1/webhooks/polar',
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'webhook-id': webhookId,
        'webhook-timestamp': timestampStr,
        'webhook-signature': signature,
      },
      body: rawBody,
    },
    opts.customEnv ?? POLAR_ENV,
  )
}

describe('Polar billing (checkout, portal, webhooks, and account deletion)', () => {
  beforeAll(async () => {
    await applyMigrations()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('creates a Polar checkout session with user metadata and rejects when Polar is unconfigured', async () => {
    const token = await createUser('usr_polar_chk', 'checkout@example.com')

    const unconfiguredEnv: Env = {
      ...TEST_ENV,
      REQUIRE_SUBSCRIPTION: 'true',
    }
    const unconfigured = await callWorker(
      'https://sync.example.com/v1/billing/polar/checkout',
      { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      unconfiguredEnv,
    )
    expect(unconfigured.status).toBe(503)
    expect(await unconfigured.json()).toMatchObject({ error: 'polar_not_configured' })

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
      new Response(JSON.stringify({ id: 'chk_123', url: 'https://sandbox.polar.sh/checkout/chk_123' }), {
        status: 201,
        headers: { 'Content-Type': 'application/json' },
      }),
    )

    const resp = await callWorker('https://sync.example.com/v1/billing/polar/checkout', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
    })
    expect(resp.status).toBe(200)
    expect(await resp.json()).toEqual({
      checkoutUrl: 'https://sandbox.polar.sh/checkout/chk_123',
    })

    expect(fetchSpy).toHaveBeenCalledTimes(1)
    const [calledUrl, calledInit] = fetchSpy.mock.calls[0]
    expect(calledUrl).toBe(`${POLAR_API_BASE}/v1/checkouts/`)
    expect(calledInit?.method).toBe('POST')
    const sentBody = JSON.parse(String(calledInit?.body))
    expect(sentBody).toEqual({
      products: ['prod_rem_premium_yearly'],
      external_customer_id: 'usr_polar_chk',
      customer_email: 'checkout@example.com',
      metadata: { user_id: 'usr_polar_chk' },
      success_url: 'https://sync.example.com/premium?billing=success',
    })
  })

  it('blocks Polar checkout for active Apple subscribers and allowlisted users', async () => {
    const appleToken = await createUser('usr_apple_sub', 'apple@example.com')
    const now = Date.now()
    await TEST_ENV.DB!.prepare(
      `INSERT INTO user_entitlements (
         user_id, status, plan, provider, provider_customer_id, provider_subscription_id,
         current_period_end, cancel_at_period_end, trial_started_at, original_transaction_id, updated_at
       ) VALUES (?1, 'active', 'premium_yearly', 'apple', NULL, 'orig_tx_1', ?2, 0, NULL, 'orig_tx_1', ?3)`,
    )
      .bind('usr_apple_sub', now + 200 * 86_400_000, now)
      .run()

    const appleBlocked = await callWorker('https://sync.example.com/v1/billing/polar/checkout', {
      method: 'POST',
      headers: { Authorization: `Bearer ${appleToken}` },
    })
    expect(appleBlocked.status).toBe(409)
    expect(await appleBlocked.json()).toMatchObject({ error: 'already_subscribed_apple' })

    const allowlistToken = await createUser('usr_allowlisted_vip', 'vip@example.com')
    const allowlistBlocked = await callWorker(
      'https://sync.example.com/v1/billing/polar/checkout',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${allowlistToken}` },
      },
      { ...POLAR_ENV, ALLOWED_EMAILS: 'vip@example.com' },
    )
    expect(allowlistBlocked.status).toBe(409)
    expect(await allowlistBlocked.json()).toMatchObject({ error: 'already_subscribed' })
  })

  it('rejects invalid signatures, tampered bodies, and stale timestamps on Polar webhooks', async () => {
    const payload = { type: 'subscription.active', data: { id: 'sub_bad', status: 'active' } }

    // Missing headers
    const missing = await callWorker('https://sync.example.com/v1/webhooks/polar', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })
    expect(missing.status).toBe(401)
    expect(await missing.json()).toMatchObject({ error: 'missing_webhook_headers' })

    // Wrong signing secret
    const wrongSig = await sendSignedPolarWebhook(payload, { secret: 'polar_whs_wrong_secret' })
    expect(wrongSig.status).toBe(401)
    expect(await wrongSig.json()).toMatchObject({ error: 'invalid_webhook_signature' })

    // Stale timestamp (> 5 minutes old)
    const stale = await sendSignedPolarWebhook(payload, {
      timestampSec: Math.floor(Date.now() / 1000) - 600,
    })
    expect(stale.status).toBe(401)
    expect(await stale.json()).toMatchObject({ error: 'webhook_timestamp_out_of_range' })

    // Future timestamp (> 5 minutes ahead)
    const future = await sendSignedPolarWebhook(payload, {
      timestampSec: Math.floor(Date.now() / 1000) + 600,
    })
    expect(future.status).toBe(401)
    expect(await future.json()).toMatchObject({ error: 'webhook_timestamp_out_of_range' })

    // Rotated multi-signature header containing an invalid v1 entry alongside a valid v1 entry succeeds
    const rawBody = JSON.stringify({ type: 'checkout.created', data: { id: 'chk_noop' } })
    const whId = 'wh_rotated_sig_1'
    const ts = String(Math.floor(Date.now() / 1000))
    const validSig = await signPolarWebhook(WEBHOOK_SECRET, whId, ts, rawBody)
    const rotatedResp = await callWorker('https://sync.example.com/v1/webhooks/polar', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'webhook-id': whId,
        'webhook-timestamp': ts,
        'webhook-signature': `v1,invalidbase64== ${validSig}`,
      },
      body: rawBody,
    })
    expect(rotatedResp.status).toBe(200)
    expect(await rotatedResp.json()).toEqual({ received: true, ignored: true })
  })

  it('handles full Polar subscription lifecycle, deduplicates retries, preserves trial_started_at, and opens portal', async () => {
    const token = await createUser('usr_polar_life', 'life@example.com')

    // User starts free trial first so trial_started_at is sticky
    const trialResp = await callWorker('https://sync.example.com/v1/trial/start', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
    })
    expect(trialResp.status).toBe(200)

    const futureEnd = new Date(Date.now() + 365 * 86_400_000).toISOString()
    const futureEndMs = Date.parse(futureEnd)

    // 1. subscription.active activates Polar subscription
    const whId1 = 'wh_life_active_1'
    const activeEvent = {
      type: 'subscription.active',
      data: {
        id: 'sub_polar_1',
        status: 'active',
        customer_id: 'cust_polar_1',
        current_period_end: futureEnd,
        cancel_at_period_end: false,
        metadata: { user_id: 'usr_polar_life' },
        customer: { id: 'cust_polar_1', external_id: 'usr_polar_life' },
      },
    }
    const resp1 = await sendSignedPolarWebhook(activeEvent, { webhookId: whId1 })
    expect(resp1.status).toBe(200)
    expect(await resp1.json()).toEqual({ received: true })

    // Duplicate delivery with same webhook-id is idempotent
    const dupResp = await sendSignedPolarWebhook(activeEvent, { webhookId: whId1 })
    expect(dupResp.status).toBe(200)
    expect(await dupResp.json()).toEqual({ received: true, duplicate: true })

    const acctActive = (await (
      await callWorker('https://sync.example.com/v1/account', {
        headers: { Authorization: `Bearer ${token}` },
      })
    ).json()) as AccountInfo
    expect(acctActive.syncEnabled).toBe(true)
    expect(acctActive.entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      provider: 'polar',
      plan: 'premium_yearly',
      expiresAt: futureEndMs,
      cancelAtPeriodEnd: false,
      canStartTrial: false,
      canSubscribePolar: false,
      canManageBilling: true,
    })

    // Portal session creation works for Polar customer
    const portalSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          id: 'cs_123',
          customer_portal_url: 'https://sandbox.polar.sh/portal/cs_123',
        }),
        { status: 201, headers: { 'Content-Type': 'application/json' } },
      ),
    )
    const portalResp = await callWorker('https://sync.example.com/v1/billing/polar/portal', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
    })
    expect(portalResp.status).toBe(200)
    expect(await portalResp.json()).toEqual({
      portalUrl: 'https://sandbox.polar.sh/portal/cs_123',
    })
    expect(portalSpy).toHaveBeenCalledTimes(1)
    expect(JSON.parse(String(portalSpy.mock.calls[0][1]?.body))).toEqual({
      customer_id: 'cust_polar_1',
    })

    // 2. subscription.updated -> past_due (grace period still active while current_period_end > now)
    const pastDueResp = await sendSignedPolarWebhook({
      type: 'subscription.updated',
      data: {
        id: 'sub_polar_1',
        status: 'past_due',
        customer_id: 'cust_polar_1',
        current_period_end: futureEnd,
        cancel_at_period_end: false,
      },
    })
    expect(pastDueResp.status).toBe(200)

    const acctPastDue = (await (
      await callWorker('https://sync.example.com/v1/account', {
        headers: { Authorization: `Bearer ${token}` },
      })
    ).json()) as AccountInfo
    expect(acctPastDue.entitlement).toMatchObject({
      state: 'past_due',
      syncEnabled: true,
      provider: 'polar',
      canManageBilling: true,
    })

    // 3. subscription.canceled with cancel_at_period_end=true keeps access until period end
    const cancelResp = await sendSignedPolarWebhook({
      type: 'subscription.canceled',
      data: {
        id: 'sub_polar_1',
        status: 'canceled',
        customer_id: 'cust_polar_1',
        current_period_end: futureEnd,
        cancel_at_period_end: true,
        ended_at: null,
      },
    })
    expect(cancelResp.status).toBe(200)

    const acctCancelScheduled = (await (
      await callWorker('https://sync.example.com/v1/account', {
        headers: { Authorization: `Bearer ${token}` },
      })
    ).json()) as AccountInfo
    expect(acctCancelScheduled.entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      cancelAtPeriodEnd: true,
      canManageBilling: true,
    })

    // 4. subscription.uncanceled clears cancel_at_period_end
    const uncancelResp = await sendSignedPolarWebhook({
      type: 'subscription.uncanceled',
      data: {
        id: 'sub_polar_1',
        status: 'active',
        customer_id: 'cust_polar_1',
        current_period_end: futureEnd,
        cancel_at_period_end: false,
      },
    })
    expect(uncancelResp.status).toBe(200)

    const acctUncanceled = (await (
      await callWorker('https://sync.example.com/v1/account', {
        headers: { Authorization: `Bearer ${token}` },
      })
    ).json()) as AccountInfo
    expect(acctUncanceled.entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      cancelAtPeriodEnd: false,
    })

    // 5. subscription.revoked immediately pauses sync and preserves sticky trial_started_at
    const revokeResp = await sendSignedPolarWebhook({
      type: 'subscription.revoked',
      data: {
        id: 'sub_polar_1',
        status: 'canceled',
        customer_id: 'cust_polar_1',
        current_period_end: futureEnd,
        ended_at: new Date().toISOString(),
        cancel_at_period_end: true,
      },
    })
    expect(revokeResp.status).toBe(200)

    const acctRevoked = (await (
      await callWorker('https://sync.example.com/v1/account', {
        headers: { Authorization: `Bearer ${token}` },
      })
    ).json()) as AccountInfo
    expect(acctRevoked.syncEnabled).toBe(false)
    expect(acctRevoked.entitlement).toMatchObject({
      state: 'expired',
      syncEnabled: false,
      provider: 'polar',
      canStartTrial: false,
      canSubscribePolar: true,
      canManageBilling: true,
    })

    // Sync endpoint now blocks with 402 state='expired'
    const blockedSync = await callWorker('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'd1', sinceSeq: 0, ops: [] }),
    })
    expect(blockedSync.status).toBe(402)
    expect(await blockedSync.json()).toMatchObject({
      error: 'subscription_required',
      state: 'expired',
    })
  })

  it('rejects portal request when user has no Polar customer and revokes Polar subscription on account delete', async () => {
    const noCustToken = await createUser('usr_no_polar', 'nopolar@example.com')
    const missingPortal = await callWorker('https://sync.example.com/v1/billing/polar/portal', {
      method: 'POST',
      headers: { Authorization: `Bearer ${noCustToken}` },
    })
    expect(missingPortal.status).toBe(404)
    expect(await missingPortal.json()).toMatchObject({ error: 'no_polar_customer' })

    const delToken = await createUser('usr_polar_delete', 'deletepolar@example.com')
    const futureEnd = new Date(Date.now() + 365 * 86_400_000).toISOString()
    await sendSignedPolarWebhook({
      type: 'subscription.active',
      data: {
        id: 'sub_polar_del_1',
        status: 'active',
        customer_id: 'cust_polar_del_1',
        current_period_end: futureEnd,
        cancel_at_period_end: false,
        customer: { id: 'cust_polar_del_1', external_id: 'usr_polar_delete' },
      },
    })

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(
      new Response(JSON.stringify({ id: 'sub_polar_del_1', status: 'canceled' }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    )

    const delResp = await callWorker('https://sync.example.com/v1/account', {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${delToken}` },
    })
    expect(delResp.status).toBe(200)
    expect(await delResp.json()).toEqual({ deleted: true })
    expect(fetchSpy).toHaveBeenCalledTimes(1)
    expect(fetchSpy.mock.calls[0][0]).toBe(`${POLAR_API_BASE}/v1/subscriptions/sub_polar_del_1`)
    expect(fetchSpy.mock.calls[0][1]?.method).toBe('DELETE')
  })

  it('serves Polar checkout and portal controls on /premium web page', async () => {
    const resp = await callWorker('https://sync.example.com/premium')
    expect(resp.status).toBe(200)
    const html = await resp.text()
    expect(html).toContain('id="btn-subscribe-polar"')
    expect(html).toContain('id="btn-manage-billing"')
    expect(html).toContain('/v1/billing/polar/checkout')
    expect(html).toContain('/v1/billing/polar/portal')
    expect(html).toContain("initialParams.get('billing') === 'success'")
  })
})
