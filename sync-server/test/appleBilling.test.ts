import { env } from 'cloudflare:test'
import { beforeAll, describe, expect, it } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import {
  APPLE_ROOT_CA_G3_DER_B64,
  decodeBase64Std,
  deriveAppleAppAccountToken,
  userIdFromReversibleAppAccountToken,
  verifyAppleJws,
} from '../src/appleBilling'
import { encodeBase64Url, sha256HexBytes, signHs256Jwt } from '../src/auth'
import worker from '../src/index'
import type { AccountInfo, Env } from '../src/types'

const TEST_ENV = env as unknown as Env
const TEXT_ENCODER = new TextEncoder()
const DAY_MS = 86_400_000

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
  while (start < value.byteLength - 1 && value[start] === 0x00) {
    start += 1
  }
  let body = value.subarray(start)
  if ((body[0] & 0x80) !== 0) {
    body = concatBytes(new Uint8Array([0x00]), body)
  }
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
  serial?: number
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
    derTlv(0xa0, derInt(new Uint8Array([0x02]))), // v3
    derInt(new Uint8Array([params.serial ?? 1])),
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
  const derSig = p1363ToDerEcdsa(rawSig)
  return derSeq(tbs, sigAlg, derBitString(derSig))
}

interface TestApplePki {
  rootDer: Uint8Array
  rootDerB64: string
  intermediateDerB64: string
  leafDerB64: string
  leafPrivateKey: CryptoKey
  intermediateKeyPair: CryptoKeyPair
  rootKeyPair: CryptoKeyPair
}

async function createTestPki(opts?: {
  leafNotBeforeMs?: number
  leafNotAfterMs?: number
  intermediateIsCa?: boolean
}): Promise<TestApplePki> {
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
    isCa: opts?.intermediateIsCa ?? true,
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
    notBeforeMs: opts?.leafNotBeforeMs ?? now - 30 * DAY_MS,
    notAfterMs: opts?.leafNotAfterMs ?? now + 365 * DAY_MS,
    serial: 3,
  })

  return {
    rootDer,
    rootDerB64: encodeBase64Std(rootDer),
    intermediateDerB64: encodeBase64Std(intermediateDer),
    leafDerB64: encodeBase64Std(leafDer),
    leafPrivateKey: leafKeyPair.privateKey,
    intermediateKeyPair,
    rootKeyPair,
  }
}

async function signTestAppleJws(
  pki: TestApplePki,
  payload: Record<string, unknown>,
  opts?: { omitRoot?: boolean },
): Promise<string> {
  const x5c = opts?.omitRoot
    ? [pki.leafDerB64, pki.intermediateDerB64]
    : [pki.leafDerB64, pki.intermediateDerB64, pki.rootDerB64]
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
     VALUES ('apple', ?1, ?2, ?3, ?4)`,
  )
    .bind(`sub_${userId}`, userId, email, now)
    .run()

  const nowSec = Math.floor(now / 1000)
  return signHs256Jwt(
    { sub: userId, typ: 'access', iat: nowSec, exp: nowSec + 900 },
    TEST_ENV.JWT_SECRET!,
  )
}

function makeAppleEnv(pki: TestApplePki, extra?: Partial<Env>): Env {
  return {
    ...TEST_ENV,
    REQUIRE_SUBSCRIPTION: 'true',
    TRIAL_DAYS: '30',
    CLOUD_PRICE_DISPLAY: '$5/year',
    POLAR_ACCESS_TOKEN: 'polar_oat_test',
    POLAR_PRODUCT_ID: 'prod_polar_test',
    APPLE_BUNDLE_ID: 'com.shettyh.rem',
    APPLE_PRODUCT_ID: 'com.shettyh.rem.premium.yearly',
    APPLE_ENVIRONMENT: 'Production,Sandbox',
    APPLE_ROOT_CA_DER_B64: pki.rootDerB64,
    ...extra,
  }
}

function callWorker(url: string, init: RequestInit | undefined, customEnv: Env): Promise<Response> {
  return worker.fetch(new Request(url, init), customEnv)
}

describe('Apple StoreKit 2 JWS verification & App Store Server Notifications v2', () => {
  let pki: TestApplePki

  beforeAll(async () => {
    await applyMigrations()
    pki = await createTestPki()
  })

  it('embeds the official Apple Root CA - G3 DER matching its known SHA-256 fingerprint', async () => {
    const der = decodeBase64Std(APPLE_ROOT_CA_G3_DER_B64)
    const sha = await sha256HexBytes(der)
    expect(sha).toBe('63343abfb89a6a03ebb57e9b3f5fa7be7c4f5c756f3017b3a8c488c3653e9179')
  })

  it('derives deterministic appAccountToken UUIDs matching iOS Swift test vectors', async () => {
    // 1. Standard 16-byte production userId (`usr_` + 22 base64url chars) is bijectively reversible
    const raw16 = new Uint8Array([
      0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
      0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
    ])
    const prodUserId = `usr_${encodeBase64Url(raw16)}`
    expect(prodUserId).toBe('usr_ASNFZ4mrze_-3LqYdlQyEA')
    const prodToken = await deriveAppleAppAccountToken(prodUserId)
    expect(prodToken).toBe('01234567-89ab-cdef-fedc-ba9876543210')
    expect(userIdFromReversibleAppAccountToken(prodToken)).toBe(prodUserId)

    // 2. Non-standard userId produces deterministic RFC 9562 UUIDv8 (verified identically in RemAppTests.swift)
    const customToken = await deriveAppleAppAccountToken('usr_test_entitlement')
    expect(customToken).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-8[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
    expect(await deriveAppleAppAccountToken('usr_test_entitlement')).toBe(customToken)
    expect(customToken).toBe('ffeedbb4-45cf-8678-85e8-487c7c392293')
  })

  it('verifies valid 3-cert and 2-cert x5c chains and rejects tampered, rogue, non-CA, and expired chains', async () => {
    const now = Date.now()
    const payload = {
      originalTransactionId: '10000001',
      bundleId: 'com.shettyh.rem',
      productId: 'com.shettyh.rem.premium.yearly',
      signedDate: now,
    }

    // Valid 3-cert chain
    const jws3 = await signTestAppleJws(pki, payload)
    const ok3 = await verifyAppleJws(jws3, undefined, { trustedRootsDer: [pki.rootDer], nowMs: now })
    expect(ok3.ok).toBe(true)

    // Valid 2-cert chain (omitting root cert in x5c)
    const jws2 = await signTestAppleJws(pki, payload, { omitRoot: true })
    const ok2 = await verifyAppleJws(jws2, undefined, { trustedRootsDer: [pki.rootDer], nowMs: now })
    expect(ok2.ok).toBe(true)

    // Tampered payload rejected
    const parts = jws3.split('.')
    const tamperedPayload = encodeBase64Url(
      TEXT_ENCODER.encode(JSON.stringify({ ...payload, originalTransactionId: '99999999' })),
    )
    const tampered = await verifyAppleJws(`${parts[0]}.${tamperedPayload}.${parts[2]}`, undefined, {
      trustedRootsDer: [pki.rootDer],
      nowMs: now,
    })
    expect(tampered).toEqual({ ok: false, error: 'invalid_jws_signature' })

    // Rogue root CA rejected against pinned root (both test root and real Apple Root CA - G3)
    const roguePki = await createTestPki()
    const rogueJws = await signTestAppleJws(roguePki, payload)
    const rogueRes = await verifyAppleJws(rogueJws, undefined, {
      trustedRootsDer: [pki.rootDer],
      nowMs: now,
    })
    expect(rogueRes).toEqual({ ok: false, error: 'x5c_untrusted_root' })

    const againstRealAppleRoot = await verifyAppleJws(jws3, undefined, { nowMs: now })
    expect(againstRealAppleRoot).toEqual({ ok: false, error: 'x5c_untrusted_root' })

    // Non-CA intermediate rejected
    const nonCaPki = await createTestPki({ intermediateIsCa: false })
    const nonCaJws = await signTestAppleJws(nonCaPki, payload)
    const nonCaRes = await verifyAppleJws(nonCaJws, undefined, {
      trustedRootsDer: [nonCaPki.rootDer],
      nowMs: now,
    })
    expect(nonCaRes).toEqual({ ok: false, error: 'x5c_chain_signature_invalid' })

    // Expired leaf certificate rejected when signedDate is after leaf expiry,
    // but accepted when signedDate fell inside the leaf validity window (restore scenario).
    const expiredLeafPki = await createTestPki({
      leafNotBeforeMs: now - 120 * DAY_MS,
      leafNotAfterMs: now - 10 * DAY_MS,
    })
    const expiredAtSign = await signTestAppleJws(expiredLeafPki, { ...payload, signedDate: now })
    expect(
      await verifyAppleJws(expiredAtSign, undefined, {
        trustedRootsDer: [expiredLeafPki.rootDer],
        nowMs: now,
      }),
    ).toEqual({ ok: false, error: 'x5c_certificate_expired' })

    const validWhenSigned = await signTestAppleJws(expiredLeafPki, {
      ...payload,
      signedDate: now - 30 * DAY_MS,
    })
    expect(
      (
        await verifyAppleJws(validWhenSigned, undefined, {
          trustedRootsDer: [expiredLeafPki.rootDer],
          nowMs: now,
        })
      ).ok,
    ).toBe(true)

    // Future signedDate rejected
    const futureSigned = await signTestAppleJws(pki, { ...payload, signedDate: now + 60 * 60 * 1000 })
    expect(
      await verifyAppleJws(futureSigned, undefined, {
        trustedRootsDer: [pki.rootDer],
        nowMs: now,
      }),
    ).toEqual({ ok: false, error: 'jws_signed_date_in_future' })
  })

  it('verifies StoreKit 2 transactions on POST /v1/billing/apple/verify and enforces bundle/product/env/ownership guards', async () => {
    const tokenA = await createUser('usr_apple_verify_a', 'apple-a@example.com')
    const tokenB = await createUser('usr_apple_verify_b', 'apple-b@example.com')
    const appleEnv = makeAppleEnv(pki)
    const now = Date.now()
    const expiresDate = now + 365 * DAY_MS

    // Unconfigured env returns 503
    const unconfiguredEnv: Env = { ...TEST_ENV, REQUIRE_SUBSCRIPTION: 'true' }
    const unconfiguredResp = await callWorker(
      'https://sync.example.com/v1/billing/apple/verify',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${tokenA}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedTransactionInfo: 'a.b.c' }),
      },
      unconfiguredEnv,
    )
    expect(unconfiguredResp.status).toBe(503)
    expect(await unconfiguredResp.json()).toMatchObject({ error: 'apple_billing_not_configured' })

    // Wrong bundleId rejected
    const wrongBundleJws = await signTestAppleJws(pki, {
      transactionId: 'tx_wrong_bundle',
      originalTransactionId: 'orig_wrong_bundle',
      bundleId: 'com.evil.other',
      productId: 'com.shettyh.rem.premium.yearly',
      environment: 'Production',
      expiresDate,
      signedDate: now,
    })
    const wrongBundleResp = await callWorker(
      'https://sync.example.com/v1/billing/apple/verify',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${tokenA}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedTransactionInfo: wrongBundleJws }),
      },
      appleEnv,
    )
    expect(wrongBundleResp.status).toBe(400)
    expect(await wrongBundleResp.json()).toMatchObject({ error: 'bundle_id_mismatch' })

    // Wrong productId rejected
    const wrongProdJws = await signTestAppleJws(pki, {
      transactionId: 'tx_wrong_prod',
      originalTransactionId: 'orig_wrong_prod',
      bundleId: 'com.shettyh.rem',
      productId: 'com.shettyh.rem.other',
      environment: 'Production',
      expiresDate,
      signedDate: now,
    })
    const wrongProdResp = await callWorker(
      'https://sync.example.com/v1/billing/apple/verify',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${tokenA}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedTransactionInfo: wrongProdJws }),
      },
      appleEnv,
    )
    expect(wrongProdResp.status).toBe(400)
    expect(await wrongProdResp.json()).toMatchObject({ error: 'product_id_mismatch' })

    // Sandbox rejected when APPLE_ENVIRONMENT is strictly 'Production'
    const sandboxJws = await signTestAppleJws(pki, {
      transactionId: 'tx_sb_1',
      originalTransactionId: 'orig_sb_1',
      bundleId: 'com.shettyh.rem',
      productId: 'com.shettyh.rem.premium.yearly',
      environment: 'Sandbox',
      expiresDate,
      signedDate: now,
    })
    const prodOnlyResp = await callWorker(
      'https://sync.example.com/v1/billing/apple/verify',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${tokenA}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedTransactionInfo: sandboxJws }),
      },
      makeAppleEnv(pki, { APPLE_ENVIRONMENT: 'Production' }),
    )
    expect(prodOnlyResp.status).toBe(400)
    expect(await prodOnlyResp.json()).toMatchObject({ error: 'environment_mismatch' })

    // Start trial first so we verify trial_started_at stays sticky after Apple upgrade
    const trialResp = await callWorker(
      'https://sync.example.com/v1/trial/start',
      { method: 'POST', headers: { Authorization: `Bearer ${tokenA}` } },
      appleEnv,
    )
    expect(trialResp.status).toBe(200)

    // Valid transaction activates Apple Premium and hides Polar subscribe
    const tokenAUuid = await deriveAppleAppAccountToken('usr_apple_verify_a')
    const validJws = await signTestAppleJws(pki, {
      transactionId: 'tx_2001',
      originalTransactionId: 'orig_2001',
      bundleId: 'com.shettyh.rem',
      productId: 'com.shettyh.rem.premium.yearly',
      environment: 'Sandbox',
      appAccountToken: tokenAUuid,
      purchaseDate: now,
      expiresDate,
      signedDate: now,
    })
    const okResp = await callWorker(
      'https://sync.example.com/v1/billing/apple/verify',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${tokenA}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedTransactionInfo: validJws }),
      },
      appleEnv,
    )
    expect(okResp.status).toBe(200)
    const acctA = (await okResp.json()) as AccountInfo
    expect(acctA.syncEnabled).toBe(true)
    expect(acctA.entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      provider: 'apple',
      plan: 'premium_yearly',
      expiresAt: expiresDate,
      cancelAtPeriodEnd: false,
      canStartTrial: false,
      canSubscribePolar: false,
      canManageBilling: false,
    })

    // Sync endpoint now succeeds
    const syncResp = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${tokenA}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'ios-1', sinceSeq: 0, ops: [] }),
      },
      appleEnv,
    )
    expect(syncResp.status).toBe(200)

    // Second account cannot hijack the same originalTransactionId
    const hijackResp = await callWorker(
      'https://sync.example.com/v1/billing/apple/verify',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${tokenB}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedTransactionInfo: validJws }),
      },
      appleEnv,
    )
    expect(hijackResp.status).toBe(409)
    expect(await hijackResp.json()).toMatchObject({ error: 'apple_transaction_already_linked' })
  })

  it('processes full ASN v2 lifecycle (SUBSCRIBED via reversible appAccountToken, renewal status, grace period, renew, expire, refund, idempotency)', async () => {
    const raw16 = new Uint8Array([
      0xaa, 0xbb, 0xcc, 0xdd, 0x11, 0x22, 0x33, 0x44,
      0x55, 0x66, 0x77, 0x88, 0x99, 0x00, 0xab, 0xcd,
    ])
    const userId = `usr_${encodeBase64Url(raw16)}`
    const userToken = await createUser(userId, 'asn-lifecycle@example.com')
    const appAccountToken = await deriveAppleAppAccountToken(userId)
    const appleEnv = makeAppleEnv(pki)
    const now = Date.now()
    const origTx = 'orig_asn_lifecycle_1'

    async function sendNotification(params: {
      notificationType: string
      subtype?: string
      notificationUUID?: string
      txOverrides?: Record<string, unknown>
      renewal?: Record<string, unknown>
    }): Promise<Response> {
      const signedTransactionInfo = await signTestAppleJws(pki, {
        transactionId: `tx_${crypto.randomUUID()}`,
        originalTransactionId: origTx,
        bundleId: 'com.shettyh.rem',
        productId: 'com.shettyh.rem.premium.yearly',
        environment: 'Production',
        appAccountToken,
        purchaseDate: now,
        expiresDate: now + 365 * DAY_MS,
        signedDate: now,
        ...params.txOverrides,
      })
      const signedRenewalInfo = params.renewal
        ? await signTestAppleJws(pki, {
            originalTransactionId: origTx,
            productId: 'com.shettyh.rem.premium.yearly',
            environment: 'Production',
            signedDate: now,
            ...params.renewal,
          })
        : undefined

      const signedPayload = await signTestAppleJws(pki, {
        notificationType: params.notificationType,
        subtype: params.subtype,
        notificationUUID: params.notificationUUID ?? crypto.randomUUID(),
        version: '2.0',
        signedDate: now,
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
        appleEnv,
      )
    }

    async function fetchAcct(): Promise<AccountInfo> {
      const r = await callWorker(
        'https://sync.example.com/v1/account',
        { headers: { Authorization: `Bearer ${userToken}` } },
        appleEnv,
      )
      expect(r.status).toBe(200)
      return (await r.json()) as AccountInfo
    }

    // 0. Invalid/untrusted outer JWS rejected with 401
    const badWebhook = await callWorker(
      'https://sync.example.com/v1/webhooks/apple',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedPayload: 'bad.jws.payload' }),
      },
      appleEnv,
    )
    expect(badWebhook.status).toBe(401)

    // 0b. TEST notification succeeds
    const testPayload = await signTestAppleJws(pki, {
      notificationType: 'TEST',
      notificationUUID: 'notif_test_1',
      version: '2.0',
      signedDate: now,
    })
    const testResp = await callWorker(
      'https://sync.example.com/v1/webhooks/apple',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedPayload: testPayload }),
      },
      appleEnv,
    )
    expect(testResp.status).toBe(200)
    expect(await testResp.json()).toEqual({ received: true, test: true })

    // 1. SUBSCRIBED resolves user purely via reversible appAccountToken (no prior /verify call!)
    const subUuid = 'notif_sub_1'
    const subResp = await sendNotification({
      notificationType: 'SUBSCRIBED',
      subtype: 'INITIAL_BUY',
      notificationUUID: subUuid,
      renewal: { autoRenewStatus: 1 },
    })
    expect(subResp.status).toBe(200)
    expect(await subResp.json()).toEqual({ received: true })

    // Duplicate notificationUUID is idempotent
    const dupResp = await sendNotification({
      notificationType: 'SUBSCRIBED',
      subtype: 'INITIAL_BUY',
      notificationUUID: subUuid,
    })
    expect(dupResp.status).toBe(200)
    expect(await dupResp.json()).toEqual({ received: true, duplicate: true })

    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      provider: 'apple',
      plan: 'premium_yearly',
      cancelAtPeriodEnd: false,
      canSubscribePolar: false,
    })

    // 2. DID_CHANGE_RENEWAL_STATUS (AUTO_RENEW_DISABLED) sets cancelAtPeriodEnd=true while keeping access
    expect(
      (
        await sendNotification({
          notificationType: 'DID_CHANGE_RENEWAL_STATUS',
          subtype: 'AUTO_RENEW_DISABLED',
          renewal: { autoRenewStatus: 0 },
        })
      ).status,
    ).toBe(200)
    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      cancelAtPeriodEnd: true,
    })

    // 2b. AUTO_RENEW_ENABLED clears cancelAtPeriodEnd
    expect(
      (
        await sendNotification({
          notificationType: 'DID_CHANGE_RENEWAL_STATUS',
          subtype: 'AUTO_RENEW_ENABLED',
          renewal: { autoRenewStatus: 1 },
        })
      ).status,
    ).toBe(200)
    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      cancelAtPeriodEnd: false,
    })

    // 3. DID_FAIL_TO_RENEW (GRACE_PERIOD) -> past_due with syncEnabled=true during grace
    const graceEnd = now + 7 * DAY_MS
    expect(
      (
        await sendNotification({
          notificationType: 'DID_FAIL_TO_RENEW',
          subtype: 'GRACE_PERIOD',
          renewal: { autoRenewStatus: 1, gracePeriodExpiresDate: graceEnd, isInBillingRetryPeriod: true },
        })
      ).status,
    ).toBe(200)
    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'past_due',
      syncEnabled: true,
      provider: 'apple',
      expiresAt: graceEnd,
    })

    // 4. GRACE_PERIOD_EXPIRED -> past_due with syncEnabled=false
    expect(
      (
        await sendNotification({
          notificationType: 'GRACE_PERIOD_EXPIRED',
          renewal: { autoRenewStatus: 1, gracePeriodExpiresDate: now - 1000 },
        })
      ).status,
    ).toBe(200)
    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'past_due',
      syncEnabled: false,
      provider: 'apple',
    })

    // 5. DID_RENEW recovers back to active with extended expiry
    const renewedEnd = now + 730 * DAY_MS
    expect(
      (
        await sendNotification({
          notificationType: 'DID_RENEW',
          subtype: 'BILLING_RECOVERY',
          txOverrides: { expiresDate: renewedEnd },
          renewal: { autoRenewStatus: 1 },
        })
      ).status,
    ).toBe(200)
    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'active',
      syncEnabled: true,
      expiresAt: renewedEnd,
      cancelAtPeriodEnd: false,
    })

    // 6. REFUND immediately pauses sync & blocks /v1/sync with 402
    expect(
      (
        await sendNotification({
          notificationType: 'REFUND',
          txOverrides: { revocationDate: now - 500, revocationReason: 0 },
        })
      ).status,
    ).toBe(200)
    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'expired',
      syncEnabled: false,
      provider: 'apple',
    })
    const blockedSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${userToken}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'ios-1', sinceSeq: 0, ops: [] }),
      },
      appleEnv,
    )
    expect(blockedSync.status).toBe(402)
    expect(await blockedSync.json()).toMatchObject({ error: 'subscription_required', state: 'expired' })

    // 7. EXPIRED keeps sync paused
    expect(
      (
        await sendNotification({
          notificationType: 'EXPIRED',
          subtype: 'VOLUNTARY',
          txOverrides: { expiresDate: now - 1000 },
        })
      ).status,
    ).toBe(200)
    expect((await fetchAcct()).entitlement).toMatchObject({
      state: 'expired',
      syncEnabled: false,
    })
  })

  it('never overwrites an active Polar subscription with a stale Apple EXPIRED/REFUND webhook', async () => {
    const userId = 'usr_polar_protected'
    const token = await createUser(userId, 'polar-prot@example.com')
    const appleEnv = makeAppleEnv(pki)
    const now = Date.now()
    const futureEnd = now + 300 * DAY_MS

    await TEST_ENV.DB!.prepare(
      `INSERT INTO user_entitlements (
         user_id, status, plan, provider, provider_customer_id, provider_subscription_id,
         current_period_end, cancel_at_period_end, trial_started_at, original_transaction_id, updated_at
       ) VALUES (?1, 'active', 'premium_yearly', 'polar', 'cust_p_1', 'sub_p_1', ?2, 0, NULL, 'orig_old_apple', ?3)`,
    )
      .bind(userId, futureEnd, now)
      .run()

    const signedTransactionInfo = await signTestAppleJws(pki, {
      transactionId: 'tx_stale_apple',
      originalTransactionId: 'orig_old_apple',
      bundleId: 'com.shettyh.rem',
      productId: 'com.shettyh.rem.premium.yearly',
      environment: 'Production',
      expiresDate: now - 1000,
      signedDate: now,
    })
    const signedPayload = await signTestAppleJws(pki, {
      notificationType: 'EXPIRED',
      notificationUUID: 'notif_stale_apple_1',
      version: '2.0',
      signedDate: now,
      data: {
        bundleId: 'com.shettyh.rem',
        environment: 'Production',
        signedTransactionInfo,
      },
    })

    const resp = await callWorker(
      'https://sync.example.com/v1/webhooks/apple',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ signedPayload }),
      },
      appleEnv,
    )
    expect(resp.status).toBe(200)
    expect(await resp.json()).toMatchObject({
      received: true,
      ignored: true,
      reason: 'active_polar_subscription',
    })

    const acct = (await (
      await callWorker(
        'https://sync.example.com/v1/account',
        { headers: { Authorization: `Bearer ${token}` } },
        appleEnv,
      )
    ).json()) as AccountInfo
    expect(acct.entitlement).toMatchObject({
      state: 'active',
      provider: 'polar',
      syncEnabled: true,
    })
  })

  it('serves Privacy Policy (/privacy), Terms of Use (/terms), and Support (/support) pages for App Store compliance', async () => {
    const appleEnv = makeAppleEnv(pki)

    const privacy = await callWorker('https://sync.example.com/privacy', undefined, appleEnv)
    expect(privacy.status).toBe(200)
    expect(privacy.headers.get('Content-Type')).toContain('text/html')
    const privacyHtml = await privacy.text()
    expect(privacyHtml).toContain('Privacy Policy')
    expect(privacyHtml).toContain('Cloud Sync &rarr; Account &rarr; Delete cloud account')
    expect(privacyHtml).toContain('mailto:manjunathshetty@live.com')
    expect(privacyHtml).not.toContain('/premium')
    expect(privacyHtml).not.toContain('/cloud')
    expect(privacyHtml).not.toMatch(/polar/i)

    const terms = await callWorker('https://sync.example.com/terms', undefined, appleEnv)
    expect(terms.status).toBe(200)
    expect(terms.headers.get('Content-Type')).toContain('text/html')
    const termsHtml = await terms.text()
    expect(termsHtml).toContain('Terms of Use')
    expect(termsHtml).toContain('stdeula')
    expect(termsHtml).toContain('mailto:manjunathshetty@live.com')
    expect(termsHtml).not.toContain('/premium')
    expect(termsHtml).not.toContain('/cloud')
    expect(termsHtml).not.toMatch(/polar/i)

    const support = await callWorker('https://sync.example.com/support', undefined, appleEnv)
    expect(support.status).toBe(200)
    expect(support.headers.get('Content-Type')).toContain('text/html')
    const supportHtml = await support.text()
    expect(supportHtml).toContain('Support')
    expect(supportHtml).toContain('mailto:manjunathshetty@live.com')
    expect(supportHtml).toContain('Restore purchases')
    expect(supportHtml).toContain('Delete cloud account')
    expect(supportHtml).not.toContain('/premium')
    expect(supportHtml).not.toContain('/cloud')
    expect(supportHtml).not.toMatch(/polar/i)
  })
})
