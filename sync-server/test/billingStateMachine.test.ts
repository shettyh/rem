import { env } from 'cloudflare:test'
import { beforeAll, describe, expect, it, vi } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import { deriveAppleAppAccountToken } from '../src/appleBilling'
import { encodeBase64Url, sha256HexBytes, signHs256Jwt } from '../src/auth'
import worker from '../src/index'
import { signPolarWebhook } from '../src/polar'
import type { AccountInfo, Env } from '../src/types'

const TEST_ENV = env as unknown as Env
const TEXT_ENCODER = new TextEncoder()
const DAY_MS = 86_400_000
const WEBHOOK_SECRET = 'polar_whs_state_machine_secret_0123456789'

// Deterministic xorshift32 PRNG for reproducible permutation tests.
function makeRng(seed: number): () => number {
  let state = seed >>> 0
  return () => {
    state ^= state << 13
    state ^= state >>> 17
    state ^= state << 5
    return (state >>> 0) / 0x100000000
  }
}

function shuffleDeterministic<T>(items: readonly T[], rng: () => number): T[] {
  const copy = [...items]
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1))
    const tmp = copy[i]
    copy[i] = copy[j]
    copy[j] = tmp
  }
  return copy
}

// ─── Minimal ASN.1 DER X.509 Test Fixture Builder (Real WebCrypto ECDSA) ─────

function encodeBase64Std(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i])
  }
  return btoa(binary)
}

function concatBytes(...chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, c) => sum + c.byteLength, 0)
  const out = new Uint8Array(total)
  let offset = 0
  for (const c of chunks) {
    out.set(c, offset)
    offset += c.byteLength
  }
  return out
}

function derLength(len: number): Uint8Array {
  if (len < 0x80) return new Uint8Array([len])
  const bytes: number[] = []
  let rem = len
  while (rem > 0) {
    bytes.unshift(rem & 0xff)
    rem >>= 8
  }
  return new Uint8Array([0x80 | bytes.length, ...bytes])
}

function derTlv(tag: number, content: Uint8Array): Uint8Array {
  return concatBytes(new Uint8Array([tag]), derLength(content.byteLength), content)
}

function derSeq(...items: Uint8Array[]): Uint8Array {
  return derTlv(0x30, concatBytes(...items))
}

function derSet(...items: Uint8Array[]): Uint8Array {
  return derTlv(0x31, concatBytes(...items))
}

function derInt(value: Uint8Array): Uint8Array {
  let start = 0
  while (start < value.byteLength - 1 && value[start] === 0x00) start += 1
  let body = value.subarray(start)
  if ((body[0] & 0x80) !== 0) body = concatBytes(new Uint8Array([0x00]), body)
  return derTlv(0x02, body)
}

function derOid(oid: string): Uint8Array {
  const parts = oid.split('.').map((p) => Number.parseInt(p, 10))
  const out: number[] = [parts[0] * 40 + parts[1]]
  for (let i = 2; i < parts.length; i++) {
    let val = parts[i]
    const stack: number[] = [val & 0x7f]
    val = Math.floor(val / 128)
    while (val > 0) {
      stack.unshift((val & 0x7f) | 0x80)
      val = Math.floor(val / 128)
    }
    out.push(...stack)
  }
  return derTlv(0x06, new Uint8Array(out))
}

function derBitString(bytes: Uint8Array): Uint8Array {
  return derTlv(0x03, concatBytes(new Uint8Array([0x00]), bytes))
}

function derGeneralizedTime(ms: number): Uint8Array {
  const d = new Date(ms)
  const pad = (n: number, len = 2) => String(n).padStart(len, '0')
  const str =
    `${pad(d.getUTCFullYear(), 4)}${pad(d.getUTCMonth() + 1)}${pad(d.getUTCDate())}` +
    `${pad(d.getUTCHours())}${pad(d.getUTCMinutes())}${pad(d.getUTCSeconds())}Z`
  return derTlv(0x18, TEXT_ENCODER.encode(str))
}

function derCnName(commonName: string): Uint8Array {
  const atv = derSeq(derOid('2.5.4.3'), derTlv(0x0c, TEXT_ENCODER.encode(commonName)))
  return derSeq(derSet(atv))
}

function p1363ToDerEcdsa(rawSig: Uint8Array): Uint8Array {
  const half = rawSig.byteLength / 2
  return derSeq(derInt(rawSig.subarray(0, half)), derInt(rawSig.subarray(half)))
}

async function generateTestCert(params: {
  subjectCn: string
  issuerCn: string
  subjectPublicKey: CryptoKey
  issuerPrivateKey: CryptoKey
  issuerCurve: 'P-256' | 'P-384'
  isCa: boolean
  notBeforeMs: number
  notAfterMs: number
  serial: number
}): Promise<Uint8Array> {
  const sigOid = params.issuerCurve === 'P-384' ? '1.2.840.10045.4.3.3' : '1.2.840.10045.4.3.2'
  const hash = params.issuerCurve === 'P-384' ? 'SHA-384' : 'SHA-256'
  const sigAlg = derSeq(derOid(sigOid))
  const spki = new Uint8Array(
    (await crypto.subtle.exportKey('spki', params.subjectPublicKey)) as ArrayBuffer,
  )
  const bcValue = params.isCa ? derSeq(derTlv(0x01, new Uint8Array([0xff]))) : derSeq()
  const bcExt = derSeq(
    derOid('2.5.29.19'),
    derTlv(0x01, new Uint8Array([0xff])),
    derTlv(0x04, bcValue),
  )
  const extensions = derTlv(0xa3, derSeq(bcExt))
  const tbs = derSeq(
    derTlv(0xa0, derInt(new Uint8Array([0x02]))),
    derInt(new Uint8Array([params.serial])),
    sigAlg,
    derCnName(params.issuerCn),
    derSeq(derGeneralizedTime(params.notBeforeMs), derGeneralizedTime(params.notAfterMs)),
    derCnName(params.subjectCn),
    spki,
    extensions,
  )
  const rawSig = new Uint8Array(
    await crypto.subtle.sign({ name: 'ECDSA', hash }, params.issuerPrivateKey, tbs),
  )
  return derSeq(tbs, sigAlg, derBitString(p1363ToDerEcdsa(rawSig)))
}

interface TestApplePki {
  rootDerB64: string
  intermediateDerB64: string
  leafDerB64: string
  leafPrivateKey: CryptoKey
}

async function createTestPki(): Promise<TestApplePki> {
  const now = Date.now()
  const rootKeyPair = (await crypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-384' },
    true,
    ['sign', 'verify'],
  )) as CryptoKeyPair
  const intermediateKeyPair = (await crypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' },
    true,
    ['sign', 'verify'],
  )) as CryptoKeyPair
  const leafKeyPair = (await crypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' },
    true,
    ['sign', 'verify'],
  )) as CryptoKeyPair

  const rootDer = await generateTestCert({
    subjectCn: 'Test Apple Root CA - G3',
    issuerCn: 'Test Apple Root CA - G3',
    subjectPublicKey: rootKeyPair.publicKey,
    issuerPrivateKey: rootKeyPair.privateKey,
    issuerCurve: 'P-384',
    isCa: true,
    notBeforeMs: now - 365 * DAY_MS,
    notAfterMs: now + 3650 * DAY_MS,
    serial: 1,
  })
  const intermediateDer = await generateTestCert({
    subjectCn: 'Test Apple WWDR CA - G6',
    issuerCn: 'Test Apple Root CA - G3',
    subjectPublicKey: intermediateKeyPair.publicKey,
    issuerPrivateKey: rootKeyPair.privateKey,
    issuerCurve: 'P-384',
    isCa: true,
    notBeforeMs: now - 365 * DAY_MS,
    notAfterMs: now + 1825 * DAY_MS,
    serial: 2,
  })
  const leafDer = await generateTestCert({
    subjectCn: 'Test Apple StoreKit Leaf',
    issuerCn: 'Test Apple WWDR CA - G6',
    subjectPublicKey: leafKeyPair.publicKey,
    issuerPrivateKey: intermediateKeyPair.privateKey,
    issuerCurve: 'P-256',
    isCa: false,
    notBeforeMs: now - 30 * DAY_MS,
    notAfterMs: now + 365 * DAY_MS,
    serial: 3,
  })

  return {
    rootDerB64: encodeBase64Std(rootDer),
    intermediateDerB64: encodeBase64Std(intermediateDer),
    leafDerB64: encodeBase64Std(leafDer),
    leafPrivateKey: leafKeyPair.privateKey,
  }
}

async function signTestAppleJws(pki: TestApplePki, payload: Record<string, unknown>): Promise<string> {
  const x5c = [pki.leafDerB64, pki.intermediateDerB64, pki.rootDerB64]
  const headerB64 = encodeBase64Url(TEXT_ENCODER.encode(JSON.stringify({ alg: 'ES256', x5c })))
  const payloadB64 = encodeBase64Url(TEXT_ENCODER.encode(JSON.stringify(payload)))
  const signingInput = TEXT_ENCODER.encode(`${headerB64}.${payloadB64}`)
  const sig = new Uint8Array(
    await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, pki.leafPrivateKey, signingInput),
  )
  return `${headerB64}.${payloadB64}.${encodeBase64Url(sig)}`
}

// ─── D1 & Worker Helpers ─────────────────────────────────────────────────────

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

function makeReversibleUserId(tag: number): string {
  const bytes = new Uint8Array(16)
  bytes[0] = 0x52
  bytes[1] = 0x45
  bytes[2] = 0x4d
  bytes[14] = (tag >> 8) & 0xff
  bytes[15] = tag & 0xff
  return `usr_${encodeBase64Url(bytes)}`
}

function callWorker(url: string, init: RequestInit | undefined, customEnv: Env): Promise<Response> {
  return worker.fetch(new Request(url, init), customEnv)
}

describe('Billing & entitlement state-machine hardening', () => {
  let pki: TestApplePki
  let billingEnv: Env

  beforeAll(async () => {
    await applyMigrations()
    pki = await createTestPki()
    billingEnv = {
      ...TEST_ENV,
      REQUIRE_SUBSCRIPTION: 'true',
      TRIAL_DAYS: '30',
      CLOUD_PRICE_DISPLAY: '$5/year',
      POLAR_ACCESS_TOKEN: 'polar_oat_test',
      POLAR_PRODUCT_ID: 'prod_polar_test',
      POLAR_WEBHOOK_SECRET: WEBHOOK_SECRET,
      POLAR_API_BASE: 'https://sandbox-api.polar.sh',
      APPLE_BUNDLE_ID: 'com.shettyh.rem',
      APPLE_PRODUCT_ID: 'com.shettyh.rem.premium.yearly',
      APPLE_ENVIRONMENT: 'Production,Sandbox',
      APPLE_ROOT_CA_DER_B64: pki.rootDerB64,
    }
  })

  async function sendPolarWebhook(
    payload: Record<string, unknown>,
    opts: { webhookId?: string; timestampSec?: number; customEnv?: Env } = {},
  ): Promise<Response> {
    const rawBody = JSON.stringify(payload)
    const webhookId = opts.webhookId ?? `wh_${crypto.randomUUID()}`
    const timestampSec = opts.timestampSec ?? Math.floor(Date.now() / 1000)
    const timestampStr = String(timestampSec)
    const signature = await signPolarWebhook(WEBHOOK_SECRET, webhookId, timestampStr, rawBody)

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
      opts.customEnv ?? billingEnv,
    )
  }

  async function sendAppleWebhook(params: {
    notificationType: string
    subtype?: string
    notificationUUID?: string
    signedDate: number
    originalTransactionId: string
    appAccountToken: string
    expiresDate: number
    revocationDate?: number
    renewal?: Record<string, unknown>
    customEnv?: Env
    signedTransactionOverride?: string
  }): Promise<Response> {
    const signedTransactionInfo =
      params.signedTransactionOverride ??
      (await signTestAppleJws(pki, {
        transactionId: `tx_${crypto.randomUUID()}`,
        originalTransactionId: params.originalTransactionId,
        bundleId: 'com.shettyh.rem',
        productId: 'com.shettyh.rem.premium.yearly',
        environment: 'Production',
        appAccountToken: params.appAccountToken,
        purchaseDate: params.signedDate,
        expiresDate: params.expiresDate,
        revocationDate: params.revocationDate,
        signedDate: params.signedDate,
      }))

    const signedRenewalInfo = params.renewal
      ? await signTestAppleJws(pki, {
          originalTransactionId: params.originalTransactionId,
          productId: 'com.shettyh.rem.premium.yearly',
          environment: 'Production',
          signedDate: params.signedDate,
          ...params.renewal,
        })
      : undefined

    const signedPayload = await signTestAppleJws(pki, {
      notificationType: params.notificationType,
      subtype: params.subtype,
      notificationUUID: params.notificationUUID ?? crypto.randomUUID(),
      version: '2.0',
      signedDate: params.signedDate,
      data: {
        bundleId: 'com.shettyh.rem',
        environment: 'Production',
        signedTransactionInfo,
        signedRenewalInfo,
      },
    })

    return callWorker(
      'https://sync.example.com/v1/webhooks/apple',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedPayload }),
      },
      params.customEnv ?? billingEnv,
    )
  }

  async function fetchAccount(token: string, customEnv = billingEnv): Promise<AccountInfo> {
    const resp = await callWorker(
      'https://sync.example.com/v1/account',
      { headers: { Authorization: `Bearer ${token}` } },
      customEnv,
    )
    expect(resp.status).toBe(200)
    return (await resp.json()) as AccountInfo
  }

  it('rejects stale/out-of-order and superseded Polar webhooks without downgrading active access', async () => {
    const userId = 'usr_polar_ooo'
    const token = await createUser(userId, 'polar-ooo@example.com')
    const nowSec = Math.floor(Date.now() / 1000)
    const nowMs = nowSec * 1000
    const period1End = new Date(nowMs + 180 * DAY_MS).toISOString()
    const period2End = new Date(nowMs + 545 * DAY_MS).toISOString()

    // Start trial first to verify trial_started_at remains sticky across all events.
    const trialResp = await callWorker(
      'https://sync.example.com/v1/trial/start',
      { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      billingEnv,
    )
    expect(trialResp.status).toBe(200)

    // Newer active subscription sub_new arrives first (period 2, newer timestamp).
    const activeResp = await sendPolarWebhook(
      {
        type: 'subscription.active',
        data: {
          id: 'sub_new',
          status: 'active',
          customer_id: 'cust_ooo',
          current_period_end: period2End,
          modified_at: new Date(nowMs).toISOString(),
          cancel_at_period_end: false,
          metadata: { user_id: userId },
        },
      },
      { timestampSec: nowSec },
    )
    expect(activeResp.status).toBe(200)

    // 1. Delayed past_due / cancel-at-period-end / revoked for superseded subscription sub_old is ignored.
    for (const staleSubEvent of [
      { type: 'subscription.updated', status: 'past_due', cancel_at_period_end: false },
      { type: 'subscription.canceled', status: 'canceled', cancel_at_period_end: true, ended_at: null },
      {
        type: 'subscription.revoked',
        status: 'canceled',
        cancel_at_period_end: true,
        ended_at: new Date(nowMs - 5000).toISOString(),
      },
    ]) {
      const resp = await sendPolarWebhook(
        {
          type: staleSubEvent.type,
          data: {
            id: 'sub_old',
            status: staleSubEvent.status,
            customer_id: 'cust_ooo',
            current_period_end: period1End,
            cancel_at_period_end: staleSubEvent.cancel_at_period_end,
            ended_at: staleSubEvent.ended_at,
            metadata: { user_id: userId },
          },
        },
        { timestampSec: nowSec },
      )
      expect(resp.status).toBe(200)
      expect(await resp.json()).toEqual({
        received: true,
        ignored: true,
        reason: 'superseded_subscription',
      })
    }

    // 2. Older-timestamp downgrade on same subscription sub_new is ignored.
    const olderTsPastDue = await sendPolarWebhook(
      {
        type: 'subscription.updated',
        data: {
          id: 'sub_new',
          status: 'past_due',
          customer_id: 'cust_ooo',
          current_period_end: period2End,
          modified_at: new Date(nowMs - 30_000).toISOString(),
          metadata: { user_id: userId },
        },
      },
      { timestampSec: nowSec - 30 },
    )
    expect(olderTsPastDue.status).toBe(200)
    expect(await olderTsPastDue.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'stale_event',
    })

    // 3. Older-period past_due on same subscription sub_new (even with equal timestamp) is ignored.
    const olderPeriodPastDue = await sendPolarWebhook(
      {
        type: 'subscription.updated',
        data: {
          id: 'sub_new',
          status: 'past_due',
          customer_id: 'cust_ooo',
          current_period_end: period1End,
          modified_at: new Date(nowMs).toISOString(),
          metadata: { user_id: userId },
        },
      },
      { timestampSec: nowSec },
    )
    expect(olderPeriodPastDue.status).toBe(200)
    expect(await olderPeriodPastDue.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'stale_event',
    })

    // Subscription stays active with period 2 expiry and sticky trial_started_at.
    const acct = await fetchAccount(token)
    expect(acct.entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      provider: 'polar',
      expiresAt: Date.parse(period2End),
      canStartTrial: false,
    })

    // 4. Legitimate newer revocation succeeds, and an older active webhook cannot resurrect it.
    const revokeResp = await sendPolarWebhook(
      {
        type: 'subscription.revoked',
        data: {
          id: 'sub_new',
          status: 'canceled',
          customer_id: 'cust_ooo',
          current_period_end: period2End,
          ended_at: new Date(nowMs + 5000).toISOString(),
          modified_at: new Date(nowMs + 5000).toISOString(),
          metadata: { user_id: userId },
        },
      },
      { timestampSec: nowSec + 5 },
    )
    expect(revokeResp.status).toBe(200)
    expect((await fetchAccount(token)).entitlement).toMatchObject({
      state: 'expired',
      syncEnabled: false,
      canStartTrial: false,
    })

    const staleResurrect = await sendPolarWebhook(
      {
        type: 'subscription.active',
        data: {
          id: 'sub_new',
          status: 'active',
          customer_id: 'cust_ooo',
          current_period_end: period2End,
          modified_at: new Date(nowMs - 10_000).toISOString(),
          metadata: { user_id: userId },
        },
      },
      { timestampSec: nowSec - 10 },
    )
    expect(staleResurrect.status).toBe(200)
    expect(await staleResurrect.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'stale_event',
    })
    expect((await fetchAccount(token)).entitlement.state).toBe('expired')
  })

  it('converges deterministically across seeded out-of-order Polar and Apple webhook permutations', async () => {
    const rng = makeRng(0x5eed1234)
    const baseSec = Math.floor(Date.now() / 1000) - 60
    const baseMs = baseSec * 1000

    // Test 6 shuffled Polar sequences: latest event (t=30s) is active renewal -> must end active.
    for (let trial = 0; trial < 6; trial++) {
      const userId = `usr_perm_polar_${trial}`
      const token = await createUser(userId, `perm-polar-${trial}@example.com`)
      await callWorker(
        'https://sync.example.com/v1/trial/start',
        { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
        billingEnv,
      )

      const timeline = [
        {
          ts: baseSec,
          body: {
            type: 'subscription.created',
            data: {
              id: `sub_perm_${trial}`,
              status: 'active',
              customer_id: `cust_perm_${trial}`,
              current_period_end: new Date(baseMs + 30 * DAY_MS).toISOString(),
              modified_at: new Date(baseMs).toISOString(),
              metadata: { user_id: userId },
            },
          },
        },
        {
          ts: baseSec + 10,
          body: {
            type: 'subscription.updated',
            data: {
              id: `sub_perm_${trial}`,
              status: 'past_due',
              customer_id: `cust_perm_${trial}`,
              current_period_end: new Date(baseMs + 30 * DAY_MS).toISOString(),
              modified_at: new Date(baseMs + 10_000).toISOString(),
              metadata: { user_id: userId },
            },
          },
        },
        {
          ts: baseSec + 20,
          body: {
            type: 'subscription.canceled',
            data: {
              id: `sub_perm_${trial}`,
              status: 'canceled',
              customer_id: `cust_perm_${trial}`,
              current_period_end: new Date(baseMs + 30 * DAY_MS).toISOString(),
              ended_at: new Date(baseMs + 20_000).toISOString(),
              modified_at: new Date(baseMs + 20_000).toISOString(),
              cancel_at_period_end: true,
              metadata: { user_id: userId },
            },
          },
        },
        {
          ts: baseSec + 30,
          body: {
            type: 'subscription.active',
            data: {
              id: `sub_perm_${trial}`,
              status: 'active',
              customer_id: `cust_perm_${trial}`,
              current_period_end: new Date(baseMs + 365 * DAY_MS).toISOString(),
              modified_at: new Date(baseMs + 30_000).toISOString(),
              cancel_at_period_end: false,
              metadata: { user_id: userId },
            },
          },
        },
      ]

      const shuffled = shuffleDeterministic(timeline, rng)
      for (const ev of shuffled) {
        const resp = await sendPolarWebhook(ev.body, { timestampSec: ev.ts })
        expect(resp.status).toBe(200)
      }

      const acct = await fetchAccount(token)
      expect(acct.entitlement).toMatchObject({
        state: 'active',
        syncEnabled: true,
        provider: 'polar',
        expiresAt: baseMs + 365 * DAY_MS,
        canStartTrial: false,
      })
    }

    // Test 6 shuffled Apple ASN v2 sequences: latest event (t=30s) is DID_RENEW -> must end active.
    for (let trial = 0; trial < 6; trial++) {
      const userId = makeReversibleUserId(100 + trial)
      const token = await createUser(userId, `perm-apple-${trial}@example.com`)
      const appAccountToken = await deriveAppleAppAccountToken(userId)
      const origTx = `orig_perm_${trial}`

      const events = [
        {
          notificationType: 'SUBSCRIBED',
          signedDate: baseMs,
          expiresDate: baseMs + 30 * DAY_MS,
          renewal: { autoRenewStatus: 1 },
        },
        {
          notificationType: 'DID_FAIL_TO_RENEW',
          subtype: 'GRACE_PERIOD',
          signedDate: baseMs + 10_000,
          expiresDate: baseMs + 30 * DAY_MS,
          renewal: { autoRenewStatus: 1, gracePeriodExpiresDate: baseMs + 35 * DAY_MS },
        },
        {
          notificationType: 'EXPIRED',
          subtype: 'BILLING_RETRY',
          signedDate: baseMs + 20_000,
          expiresDate: baseMs + 30 * DAY_MS,
          renewal: { autoRenewStatus: 0 },
        },
        {
          notificationType: 'DID_RENEW',
          subtype: 'BILLING_RECOVERY',
          signedDate: baseMs + 30_000,
          expiresDate: baseMs + 395 * DAY_MS,
          renewal: { autoRenewStatus: 1 },
        },
      ]

      for (const ev of shuffleDeterministic(events, rng)) {
        const resp = await sendAppleWebhook({
          notificationType: ev.notificationType,
          subtype: ev.subtype,
          signedDate: ev.signedDate,
          originalTransactionId: origTx,
          appAccountToken,
          expiresDate: ev.expiresDate,
          renewal: ev.renewal,
        })
        expect(resp.status).toBe(200)
      }

      const acct = await fetchAccount(token)
      expect(acct.entitlement).toMatchObject({
        state: 'active',
        syncEnabled: true,
        provider: 'apple',
        expiresAt: baseMs + 395 * DAY_MS,
      })
    }
  })

  it('rejects stale/superseded Apple webhooks and guards active subscriptions across providers including past_due', async () => {
    const now = Date.now()
    const futureEnd = now + 300 * DAY_MS

    // A. Apple active user protected against Polar past_due & canceled, and superseded Apple tx.
    const appleUserId = makeReversibleUserId(201)
    const appleToken = await createUser(appleUserId, 'cross-apple@example.com')
    const appleAccountUuid = await deriveAppleAppAccountToken(appleUserId)

    const subResp = await sendAppleWebhook({
      notificationType: 'SUBSCRIBED',
      signedDate: now,
      originalTransactionId: 'orig_curr_apple',
      appAccountToken: appleAccountUuid,
      expiresDate: futureEnd,
      renewal: { autoRenewStatus: 1 },
    })
    expect(subResp.status).toBe(200)

    // Superseded Apple originalTransactionId downgrades (past_due and cancel-at-period-end) are ignored.
    const oldTxFail = await sendAppleWebhook({
      notificationType: 'DID_FAIL_TO_RENEW',
      signedDate: now + 1000,
      originalTransactionId: 'orig_old_apple',
      appAccountToken: appleAccountUuid,
      expiresDate: now - 1000,
    })
    expect(oldTxFail.status).toBe(200)
    expect(await oldTxFail.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'superseded_transaction',
    })

    const oldTxCancelAtEnd = await sendAppleWebhook({
      notificationType: 'DID_CHANGE_RENEWAL_STATUS',
      subtype: 'AUTO_RENEW_DISABLED',
      signedDate: now + 1000,
      originalTransactionId: 'orig_old_apple',
      appAccountToken: appleAccountUuid,
      expiresDate: now + 30 * DAY_MS,
      renewal: { autoRenewStatus: 0 },
    })
    expect(oldTxCancelAtEnd.status).toBe(200)
    expect(await oldTxCancelAtEnd.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'superseded_transaction',
    })

    // Older signedDate REFUND on current tx is ignored as stale.
    const staleRefund = await sendAppleWebhook({
      notificationType: 'REFUND',
      signedDate: now - 5000,
      originalTransactionId: 'orig_curr_apple',
      appAccountToken: appleAccountUuid,
      expiresDate: futureEnd,
      revocationDate: now - 5000,
    })
    expect(staleRefund.status).toBe(200)
    expect(await staleRefund.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'stale_event',
    })

    // Polar past_due, cancel-at-period-end, AND revoked must NOT overwrite active Apple subscription.
    const polarPastDue = await sendPolarWebhook({
      type: 'subscription.updated',
      data: {
        id: 'sub_old_polar',
        status: 'past_due',
        customer_id: 'cust_old_polar',
        current_period_end: new Date(now + 10 * DAY_MS).toISOString(),
        metadata: { user_id: appleUserId },
      },
    })
    expect(polarPastDue.status).toBe(200)
    expect(await polarPastDue.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'active_apple_subscription',
    })

    const polarCancelAtPeriodEnd = await sendPolarWebhook({
      type: 'subscription.canceled',
      data: {
        id: 'sub_old_polar',
        status: 'canceled',
        customer_id: 'cust_old_polar',
        current_period_end: new Date(now + 10 * DAY_MS).toISOString(),
        cancel_at_period_end: true,
        ended_at: null,
        metadata: { user_id: appleUserId },
      },
    })
    expect(polarCancelAtPeriodEnd.status).toBe(200)
    expect(await polarCancelAtPeriodEnd.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'active_apple_subscription',
    })

    expect((await fetchAccount(appleToken)).entitlement).toMatchObject({
      state: 'active',
      provider: 'apple',
      syncEnabled: true,
      cancelAtPeriodEnd: false,
    })

    // B. Polar active user protected against Apple past_due, cancel-at-period-end, and EXPIRED.
    const polarUserId = makeReversibleUserId(202)
    const polarToken = await createUser(polarUserId, 'cross-polar@example.com')
    const polarUuid = await deriveAppleAppAccountToken(polarUserId)

    const polarActive = await sendPolarWebhook({
      type: 'subscription.active',
      data: {
        id: 'sub_curr_polar',
        status: 'active',
        customer_id: 'cust_curr_polar',
        current_period_end: new Date(futureEnd).toISOString(),
        metadata: { user_id: polarUserId },
      },
    })
    expect(polarActive.status).toBe(200)

    for (const notifType of ['DID_FAIL_TO_RENEW', 'GRACE_PERIOD_EXPIRED', 'EXPIRED']) {
      const resp = await sendAppleWebhook({
        notificationType: notifType,
        signedDate: now + 1000,
        originalTransactionId: 'orig_stale_cross_apple',
        appAccountToken: polarUuid,
        expiresDate: now - 1000,
      })
      expect(resp.status).toBe(200)
      expect(await resp.json()).toEqual({
        received: true,
        ignored: true,
        reason: 'active_polar_subscription',
      })
    }

    const appleCancelAtPeriodEnd = await sendAppleWebhook({
      notificationType: 'DID_CHANGE_RENEWAL_STATUS',
      subtype: 'AUTO_RENEW_DISABLED',
      signedDate: now + 1000,
      originalTransactionId: 'orig_stale_cross_apple',
      appAccountToken: polarUuid,
      expiresDate: now + 20 * DAY_MS,
      renewal: { autoRenewStatus: 0 },
    })
    expect(appleCancelAtPeriodEnd.status).toBe(200)
    expect(await appleCancelAtPeriodEnd.json()).toEqual({
      received: true,
      ignored: true,
      reason: 'active_polar_subscription',
    })

    expect((await fetchAccount(polarToken)).entitlement).toMatchObject({
      state: 'active',
      provider: 'polar',
      syncEnabled: true,
      cancelAtPeriodEnd: false,
    })
  })

  it('commits billing_events atomically with user_entitlements so failed writes retry cleanly', async () => {
    const now = Date.now()
    const futureEnd = now + 365 * DAY_MS

    function makeFailOnceBatchDb(realDb: D1Database): D1Database {
      let failNextBatch = true
      return new Proxy(realDb, {
        get(target, prop, receiver) {
          if (prop === 'batch') {
            return async (statements: D1PreparedStatement[]) => {
              if (failNextBatch) {
                failNextBatch = false
                throw new Error('simulated transient D1 batch failure')
              }
              return target.batch(statements)
            }
          }
          const val = Reflect.get(target, prop, receiver)
          return typeof val === 'function' ? val.bind(target) : val
        },
      })
    }

    // 1. Polar webhook atomic retry after batch failure
    const polarUser = 'usr_atomic_polar'
    const polarToken = await createUser(polarUser, 'atomic-polar@example.com')
    const polarWhId = 'wh_atomic_retry_1'
    const polarPayload = {
      type: 'subscription.active',
      data: {
        id: 'sub_atomic_1',
        status: 'active',
        customer_id: 'cust_atomic_1',
        current_period_end: new Date(futureEnd).toISOString(),
        metadata: { user_id: polarUser },
      },
    }
    const flakyPolarEnv: Env = { ...billingEnv, DB: makeFailOnceBatchDb(TEST_ENV.DB!) }

    await expect(
      sendPolarWebhook(polarPayload, { webhookId: polarWhId, customEnv: flakyPolarEnv }),
    ).rejects.toThrow('simulated transient D1 batch failure')

    // billing_events must NOT have recorded the failed webhook.
    const polarEventAfterFail = await TEST_ENV.DB!.prepare(
      `SELECT event_id FROM billing_events WHERE provider = 'polar' AND event_id = ?1`,
    )
      .bind(polarWhId)
      .first()
    expect(polarEventAfterFail).toBeNull()

    // Retry with identical webhookId now succeeds and activates entitlement.
    const polarRetry = await sendPolarWebhook(polarPayload, {
      webhookId: polarWhId,
      customEnv: flakyPolarEnv,
    })
    expect(polarRetry.status).toBe(200)
    expect(await polarRetry.json()).toEqual({ received: true })
    expect((await fetchAccount(polarToken)).entitlement.state).toBe('active')

    // Subsequent duplicate delivery returns duplicate: true.
    const polarDup = await sendPolarWebhook(polarPayload, {
      webhookId: polarWhId,
      customEnv: flakyPolarEnv,
    })
    expect(polarDup.status).toBe(200)
    expect(await polarDup.json()).toEqual({ received: true, duplicate: true })

    // 2. Apple webhook atomic retry after batch failure + bad inner JWS never poisons billing_events
    const appleUser = makeReversibleUserId(301)
    const appleToken = await createUser(appleUser, 'atomic-apple@example.com')
    const appleUuid = await deriveAppleAppAccountToken(appleUser)
    const notifUuid = 'notif_atomic_retry_1'

    // First send an invalid inner signedTransactionInfo with this UUID -> 401 and NOT recorded.
    const badInner = await sendAppleWebhook({
      notificationType: 'SUBSCRIBED',
      notificationUUID: notifUuid,
      signedDate: now,
      originalTransactionId: 'orig_atomic_1',
      appAccountToken: appleUuid,
      expiresDate: futureEnd,
      signedTransactionOverride: 'not.a.validjws',
    })
    expect(badInner.status).toBe(401)
    const appleEventAfterBadInner = await TEST_ENV.DB!.prepare(
      `SELECT event_id FROM billing_events WHERE provider = 'apple' AND event_id = ?1`,
    )
      .bind(notifUuid)
      .first()
    expect(appleEventAfterBadInner).toBeNull()

    // Now simulate transient D1 batch failure on valid webhook -> throws, no billing_events row.
    const flakyAppleEnv: Env = { ...billingEnv, DB: makeFailOnceBatchDb(TEST_ENV.DB!) }
    await expect(
      sendAppleWebhook({
        notificationType: 'SUBSCRIBED',
        notificationUUID: notifUuid,
        signedDate: now,
        originalTransactionId: 'orig_atomic_1',
        appAccountToken: appleUuid,
        expiresDate: futureEnd,
        customEnv: flakyAppleEnv,
      }),
    ).rejects.toThrow('simulated transient D1 batch failure')

    const appleEventAfterFail = await TEST_ENV.DB!.prepare(
      `SELECT event_id FROM billing_events WHERE provider = 'apple' AND event_id = ?1`,
    )
      .bind(notifUuid)
      .first()
    expect(appleEventAfterFail).toBeNull()

    // Retry with identical notificationUUID succeeds and activates entitlement.
    const appleRetry = await sendAppleWebhook({
      notificationType: 'SUBSCRIBED',
      notificationUUID: notifUuid,
      signedDate: now,
      originalTransactionId: 'orig_atomic_1',
      appAccountToken: appleUuid,
      expiresDate: futureEnd,
      customEnv: flakyAppleEnv,
    })
    expect(appleRetry.status).toBe(200)
    expect(await appleRetry.json()).toEqual({ received: true })
    expect((await fetchAccount(appleToken)).entitlement).toMatchObject({
      state: 'active',
      provider: 'apple',
    })
  })

  it('rejects stale JWTs after DELETE /v1/account across account, trial, billing, sync, and asset routes', async () => {
    const userId = 'usr_deleted_ghost'
    const token = await createUser(userId, 'ghost@example.com')

    // Start trial and write an op + asset before deleting the account.
    expect(
      (
        await callWorker(
          'https://sync.example.com/v1/trial/start',
          { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
          billingEnv,
        )
      ).status,
    ).toBe(200)

    const assetBytes = TEXT_ENCODER.encode('ghost-asset-payload')
    const assetHash = await sha256HexBytes(assetBytes)
    expect(
      (
        await callWorker(
          `https://sync.example.com/v1/assets/${assetHash}`,
          {
            method: 'PUT',
            headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'image/png' },
            body: assetBytes,
          },
          billingEnv,
        )
      ).status,
    ).toBe(200)

    // Delete account.
    const delResp = await callWorker(
      'https://sync.example.com/v1/account',
      { method: 'DELETE', headers: { Authorization: `Bearer ${token}` } },
      billingEnv,
    )
    expect(delResp.status).toBe(200)

    const polarFetchSpy = vi.spyOn(globalThis, 'fetch')
    const selfHostedEnv: Env = { ...billingEnv, REQUIRE_SUBSCRIPTION: 'false' }

    const calls: Array<{ name: string; env: Env; url: string; init: RequestInit }> = [
      {
        name: 'GET /v1/account',
        env: billingEnv,
        url: 'https://sync.example.com/v1/account',
        init: { headers: { Authorization: `Bearer ${token}` } },
      },
      {
        name: 'POST /v1/trial/start',
        env: billingEnv,
        url: 'https://sync.example.com/v1/trial/start',
        init: { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      },
      {
        name: 'POST /v1/billing/polar/checkout',
        env: billingEnv,
        url: 'https://sync.example.com/v1/billing/polar/checkout',
        init: { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      },
      {
        name: 'POST /v1/billing/polar/portal',
        env: billingEnv,
        url: 'https://sync.example.com/v1/billing/polar/portal',
        init: { method: 'POST', headers: { Authorization: `Bearer ${token}` } },
      },
      {
        name: 'POST /v1/billing/apple/verify',
        env: billingEnv,
        url: 'https://sync.example.com/v1/billing/apple/verify',
        init: {
          method: 'POST',
          headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
          body: JSON.stringify({ signedTransactionInfo: 'a.b.c' }),
        },
      },
      {
        name: 'POST /v1/sync (self-hosted mode)',
        env: selfHostedEnv,
        url: 'https://sync.example.com/v1/sync',
        init: {
          method: 'POST',
          headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
          body: JSON.stringify({
            deviceId: 'dev-ghost',
            sinceSeq: 0,
            ops: [
              {
                opId: 'ghost-op-1',
                payload: { type: 'deckUpsert', id: 'd1', hlc: '0000000001000-0000-aaaaaaaa' },
              },
            ],
          }),
        },
      },
      {
        name: 'POST /v1/assets/missing (self-hosted mode)',
        env: selfHostedEnv,
        url: 'https://sync.example.com/v1/assets/missing',
        init: {
          method: 'POST',
          headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
          body: JSON.stringify({ hashes: [assetHash] }),
        },
      },
      {
        name: 'PUT /v1/assets/:hash (self-hosted mode)',
        env: selfHostedEnv,
        url: `https://sync.example.com/v1/assets/${assetHash}`,
        init: {
          method: 'PUT',
          headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'image/png' },
          body: assetBytes,
        },
      },
    ]

    for (const c of calls) {
      const resp = await callWorker(c.url, c.init, c.env)
      expect(resp.status, `${c.name} should return 401`).toBe(401)
      expect(await resp.json(), `${c.name} error`).toEqual({ error: 'account_not_found' })
    }

    // Polar external checkout/portal API was never invoked, and no orphan R2/DO data was recreated.
    expect(polarFetchSpy).not.toHaveBeenCalled()
    expect(await TEST_ENV.ASSETS.get(`users/${userId}/assets/${assetHash}`)).toBeNull()
    const doCheck = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${TEST_ENV.DEV_AUTH_TOKEN}:${userId}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ deviceId: 'dev-check', sinceSeq: 0, ops: [] }),
      },
      selfHostedEnv,
    )
    expect(await doCheck.json()).toEqual({
      latestSeq: 0,
      hasMore: false,
      ackedOpIds: [],
      ops: [],
    })
  })
})
