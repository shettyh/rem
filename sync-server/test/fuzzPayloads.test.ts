import { env } from 'cloudflare:test'
import { beforeAll, describe, expect, it } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import { verifyAppleJws } from '../src/appleBilling'
import { encodeBase64Url } from '../src/auth'
import worker from '../src/index'
import type { Env } from '../src/types'

const TEST_ENV = env as unknown as Env
const TEXT_ENCODER = new TextEncoder()
const DEV_SECRET = 'test-dev-secret'

// Deterministic xorshift32 PRNG for reproducible fuzzing.
function makeRng(seed: number): () => number {
  let state = seed >>> 0
  return () => {
    state ^= state << 13
    state ^= state >>> 17
    state ^= state << 5
    return state >>> 0
  }
}

function encodeBase64Std(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i])
  }
  return btoa(binary)
}

function callWorker(url: string, init: RequestInit, customEnv: Env = TEST_ENV): Promise<Response> {
  return worker.fetch(new Request(url, init), customEnv)
}

function authHeader(userId: string): Record<string, string> {
  return { Authorization: `Bearer ${DEV_SECRET}:${userId}` }
}

function makeFakeJws(header: unknown, payload: unknown, sigBytes = new Uint8Array(64).fill(7)): string {
  const h = encodeBase64Url(TEXT_ENCODER.encode(JSON.stringify(header)))
  const p = encodeBase64Url(TEXT_ENCODER.encode(JSON.stringify(payload)))
  const s = encodeBase64Url(sigBytes)
  return `${h}.${p}.${s}`
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

describe('Seeded malformed-payload & ASN.1 DER fuzz tests', () => {
  const appleEnv: Env = {
    ...TEST_ENV,
    APPLE_BUNDLE_ID: 'com.shettyh.rem',
    APPLE_PRODUCT_ID: 'com.shettyh.rem.premium.yearly',
    APPLE_ENVIRONMENT: 'Production,Sandbox',
  }

  beforeAll(async () => {
    await applyMigrations()
  })

  it('fuzzes ASN.1 DER / X.509 / JWS verification without uncaught throws, hangs, or 500s', async () => {
    const rng = makeRng(0xdecafbad)
    const nowMs = Date.now()

    // Construct targeted pathological DER payloads.
    const deeplyNested = (() => {
      let cur = new Uint8Array([0x05, 0x00])
      for (let i = 0; i < 64; i++) {
        const len = cur.byteLength
        const header = len < 0x80 ? new Uint8Array([0x30, len]) : new Uint8Array([0x30, 0x82, (len >> 8) & 0xff, len & 0xff])
        const next = new Uint8Array(header.byteLength + len)
        next.set(header, 0)
        next.set(cur, header.byteLength)
        cur = next
      }
      return cur
    })()

    const overflowOidCert = new Uint8Array([
      0x30, 0x24,
      0x30, 0x04, 0x05, 0x00, 0x05, 0x00,
      0x30, 0x16,
      0x06, 0x14, 0x2a,
      0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81,
      0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x01,
      0x03, 0x04, 0x00, 0x30, 0x00, 0x00,
    ])

    const pathologicalDers: Uint8Array[] = [
      new Uint8Array([]),
      new Uint8Array([0x30]),
      new Uint8Array([0x30, 0x80, 0x00, 0x00]), // indefinite length
      new Uint8Array([0x30, 0x84, 0xff, 0xff, 0xff, 0xff, 0x00]), // signed-32bit negative length
      new Uint8Array([0x30, 0x84, 0x7f, 0xff, 0xff, 0xff, 0x00]), // huge positive length
      new Uint8Array([0x30, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00]), // >4 length octets
      new Uint8Array([0x30, 0x06, 0x30, 0x00, 0x30, 0x00, 0x03, 0x01]), // truncated bitstring
      new Uint8Array([0x30, 0x08, 0x30, 0x00, 0x30, 0x00, 0x03, 0x02, 0x01, 0x00]), // non-zero unused bits
      new Uint8Array(20 * 1024).fill(0x30), // >16 KiB oversized cert
      deeplyNested,
      overflowOidCert,
    ]

    // Generate 80 seeded random/mutated DER buffers.
    for (let i = 0; i < 80; i++) {
      const len = (rng() % 256) + 1
      const buf = new Uint8Array(len)
      for (let j = 0; j < len; j++) {
        buf[j] = rng() & 0xff
      }
      // Bias half of samples to start with SEQUENCE tag so parser enters deeper branches.
      if (i % 2 === 0) {
        buf[0] = 0x30
        if (len > 1) buf[1] = Math.min(len - 2, 0x7f)
      }
      pathologicalDers.push(buf)
    }

    // Wrap each DER buffer in a JWS x5c chain and assert clean { ok: false, error }.
    for (const der of pathologicalDers) {
      const b64 = encodeBase64Std(der)
      const jws = makeFakeJws(
        { alg: 'ES256', x5c: [b64, b64] },
        { signedDate: nowMs, notificationType: 'SUBSCRIBED', notificationUUID: 'fuzz-uuid' },
      )
      const res = await verifyAppleJws(jws, appleEnv, { nowMs })
      expect(res.ok).toBe(false)
      if (!res.ok) {
        expect(res.error.length).toBeGreaterThan(0)
      }
    }

    // Malformed JWS header/payload envelopes (null, arrays, primitives, bad x5c, bad signatures).
    const malformedJwsList: string[] = [
      '',
      'only-one-part',
      'a.b',
      'a.b.c.d',
      '!!!.@@@.###',
      makeFakeJws(null, null),
      makeFakeJws([], []),
      makeFakeJws('str', 123),
      makeFakeJws({ alg: 'RS256', x5c: ['AA==', 'AA=='] }, { signedDate: nowMs }),
      makeFakeJws({ alg: 'ES256', x5c: ['AA=='] }, { signedDate: nowMs }), // chain < 2
      makeFakeJws({ alg: 'ES256', x5c: ['AA==', 'AA==', 'AA==', 'AA==', 'AA==', 'AA=='] }, { signedDate: nowMs }), // chain > 5
      makeFakeJws({ alg: 'ES256', x5c: [123, null] }, { signedDate: nowMs }),
      makeFakeJws({ alg: 'ES256', x5c: ['***invalid-b64***', 'AA=='] }, { signedDate: nowMs }),
      makeFakeJws({ alg: 'ES256', x5c: ['AA==', 'AA=='] }, { signedDate: nowMs }, new Uint8Array(63)), // bad sig len
      makeFakeJws({ alg: 'ES256', x5c: ['AA==', 'AA=='] }, { signedDate: nowMs + 60 * 60 * 1000 }), // future signedDate
    ]

    for (const jws of malformedJwsList) {
      const res = await verifyAppleJws(jws, appleEnv, { nowMs })
      expect(res.ok).toBe(false)

      // Also verify POST /v1/webhooks/apple never 500s on any of these.
      const httpResp = await callWorker(
        'https://sync.example.com/v1/webhooks/apple',
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ signedPayload: jws }),
        },
        appleEnv,
      )
      expect([400, 401]).toContain(httpResp.status)
    }

    // Malformed Bearer JWT with `null` header/payload must return 401, never 500.
    const nullJwt = makeFakeJws(null, null)
    const nullJwtResp = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${nullJwt}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
      },
      TEST_ENV,
    )
    expect(nullJwtResp.status).toBe(401)
  })

  it('rejects malformed, bad-HLC, and oversized /v1/sync & /v1/assets/missing requests with 400 and writes zero DO rows', async () => {
    const user = 'fuzz-sync-user'
    const rng = makeRng(0x1337c0de)

    const validHlc = '0000000001000-0000-aaaaaaaa'
    const badHlcs: unknown[] = [
      '',
      'bad',
      '000000000100-0000-aaaaaaaa', // 12-digit ms
      '00000000010000-0000-aaaaaaaa', // 14-digit ms
      '0000000001000-000-aaaaaaaa', // 3-digit counter
      '0000000001000-0000-AAAAAAA1', // uppercase device id
      '0000000001000-0000-aaaaaaa', // 7-char device id
      '0000000001000-0000-aaaaaaaaa', // 9-char device id
      '0000000001000_0000_aaaaaaaa', // wrong separator
      '000000000100a-0000-aaaaaaaa', // non-digit ms
      '0000000001000-0000-aaaa_aaa', // non-alnum device id
      '0000000001000-0000-aaaaaaaa\n', // trailing newline
      1234567890123,
      null,
      {},
    ]

    const badSyncBodies: string[] = [
      '',
      '{not-json',
      'null',
      '[]',
      '"hello"',
      '42',
      JSON.stringify({}),
      JSON.stringify({ deviceId: '', sinceSeq: 0, ops: [] }),
      JSON.stringify({ deviceId: '   ', sinceSeq: 0, ops: [] }),
      JSON.stringify({ deviceId: 'x'.repeat(65), sinceSeq: 0, ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: -1, ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 1.5, ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 1e30, ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: '0', ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, limit: 0, ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, limit: -5, ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, limit: 1.2, ops: [] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: 'not-an-array' }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [null] }),
      JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [[]] }),
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: [{ opId: '', payload: { type: 'deckUpsert', hlc: validHlc } }],
      }),
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: [{ opId: 'x'.repeat(129), payload: { type: 'deckUpsert', hlc: validHlc } }],
      }),
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: [{ opId: 'op-arr-payload', payload: [] }],
      }),
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: [{ opId: 'op-blank-type', payload: { type: '   ', hlc: validHlc } }],
      }),
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: [{ opId: 'op-long-type', payload: { type: 't'.repeat(65), hlc: validHlc } }],
      }),
      // Single op exceeding 256 KiB
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: [
          {
            opId: 'op-huge-1',
            payload: { type: 'cardCreate', hlc: validHlc, front: 'A'.repeat(270 * 1024) },
          },
        ],
      }),
      // Batch exceeding 2 MiB across valid-sized ops (10 ops * 220 KiB = 2.2 MiB)
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: Array.from({ length: 10 }, (_, idx) => ({
          opId: `op-batch-huge-${idx}`,
          payload: { type: 'cardCreate', hlc: validHlc, front: 'B'.repeat(220 * 1024) },
        })),
      }),
      // Valid op followed by an invalid op in the SAME batch -> atomically rejected (zero rows written!)
      JSON.stringify({
        deviceId: 'dev1',
        sinceSeq: 0,
        ops: [
          { opId: 'op-good-first', payload: { type: 'deckUpsert', id: 'd1', hlc: validHlc } },
          { opId: 'op-bad-second', payload: { type: 'deckUpsert', id: 'd2', hlc: 'not-an-hlc' } },
        ],
      }),
    ]

    for (const hlc of badHlcs) {
      badSyncBodies.push(
        JSON.stringify({
          deviceId: 'dev1',
          sinceSeq: 0,
          ops: [{ opId: `op-bad-hlc-${String(hlc)}`, payload: { type: 'deckUpsert', hlc } }],
        }),
      )
    }

    // Add 60 seeded randomly mutated sync requests.
    const randomValues: unknown[] = [null, true, false, -1, 0, 1.5, '', 'bad', [], {}, { type: 'x', hlc: 'bad' }]
    for (let i = 0; i < 60; i++) {
      const pick = () => randomValues[rng() % randomValues.length]
      const mutated = {
        deviceId: i % 3 === 0 ? 'dev1' : pick(),
        sinceSeq: i % 4 === 0 ? 0 : pick(),
        limit: i % 5 === 0 ? pick() : 10,
        ops:
          i % 2 === 0
            ? [{ opId: i % 6 === 0 ? `op-${i}` : pick(), payload: pick() }]
            : pick(),
      }
      badSyncBodies.push(JSON.stringify(mutated))
    }

    for (const body of badSyncBodies) {
      const resp = await callWorker('https://sync.example.com/v1/sync', {
        method: 'POST',
        headers: { ...authHeader(user), 'Content-Type': 'application/json' },
        body,
      })
      expect(resp.status, `expected 400 for body prefix: ${body.slice(0, 120)}`).toBe(400)
    }

    // Zero-write invariant: DO must still be completely empty after all rejected requests.
    const cleanPull = await callWorker('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'dev-verify', sinceSeq: 0, ops: [] }),
    })
    expect(cleanPull.status).toBe(200)
    expect(await cleanPull.json()).toEqual({
      latestSeq: 0,
      hasMore: false,
      ackedOpIds: [],
      ops: [],
    })

    // Malformed /v1/assets/missing payloads must always return 400 (never 500).
    const badMissingBodies: string[] = [
      '',
      '{bad',
      'null',
      '[]',
      JSON.stringify({}),
      JSON.stringify({ hashes: 'not-array' }),
      JSON.stringify({ hashes: ['not-a-sha256'] }),
      JSON.stringify({ hashes: ['A'.repeat(64)] }), // uppercase hex rejected
      JSON.stringify({ hashes: [123] }),
      JSON.stringify({ hashes: Array.from({ length: 501 }, () => 'a'.repeat(64)) }), // > 500
    ]
    for (const body of badMissingBodies) {
      const resp = await callWorker('https://sync.example.com/v1/assets/missing', {
        method: 'POST',
        headers: { ...authHeader(user), 'Content-Type': 'application/json' },
        body,
      })
      expect(resp.status).toBe(400)
    }
  })

  it('returns 500 internal_error when UserSyncDO throws an unexpected runtime error (not 400)', async () => {
    const brokenDoEnv: Env = {
      ...TEST_ENV,
      USER_SYNC: {
        idFromName: TEST_ENV.USER_SYNC.idFromName.bind(TEST_ENV.USER_SYNC),
        get: () =>
          ({
            sync: async () => {
              throw new Error('SQLITE_BUSY: durable object storage temporarily unavailable')
            },
          }) as unknown as ReturnType<Env['USER_SYNC']['get']>,
      } as unknown as Env['USER_SYNC'],
    }

    const resp = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { ...authHeader('do-error-user'), 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
      },
      brokenDoEnv,
    )
    expect(resp.status).toBe(500)
    expect(await resp.json()).toEqual({
      error: 'internal_error',
      message: 'sync storage error',
    })
  })

  it('returns structured 503/500 JSON (never uncaught 1101) when DO or R2 throws on sync and asset routes', async () => {
    const quotaError = new Error('Exceeded allowed rows read in Durable Objects free tier.')
    const quotaBrokenEnv: Env = {
      ...TEST_ENV,
      USER_SYNC: {
        idFromName: TEST_ENV.USER_SYNC.idFromName.bind(TEST_ENV.USER_SYNC),
        get: () =>
          ({
            sync: async () => {
              throw quotaError
            },
            missingAssets: async () => {
              throw quotaError
            },
            checkAssetQuota: async () => {
              throw quotaError
            },
          }) as unknown as ReturnType<Env['USER_SYNC']['get']>,
      } as unknown as Env['USER_SYNC'],
    }

    const bytes = new Uint8Array([1, 2, 3, 4])
    const digest = await crypto.subtle.digest('SHA-256', bytes)
    const hash = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')

    const quotaBody = {
      error: 'storage_limit_exceeded',
      message: 'sync storage temporarily unavailable (server storage limit exceeded)',
    }
    const internalBody = {
      error: 'internal_error',
      message: 'sync storage error',
    }

    // POST /v1/sync -> 503 storage_limit_exceeded with Cache-Control & Retry-After
    const syncQuotaResp = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: { ...authHeader('quota-err-user'), 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
      },
      quotaBrokenEnv,
    )
    expect(syncQuotaResp.status).toBe(503)
    expect(syncQuotaResp.headers.get('Cache-Control')).toBe('no-store')
    expect(syncQuotaResp.headers.get('Retry-After')).toBe('60')
    expect(await syncQuotaResp.json()).toEqual(quotaBody)

    // POST /v1/assets/missing -> 503 storage_limit_exceeded
    const missingQuotaResp = await callWorker(
      'https://sync.example.com/v1/assets/missing',
      {
        method: 'POST',
        headers: { ...authHeader('quota-err-user'), 'Content-Type': 'application/json' },
        body: JSON.stringify({ hashes: [hash] }),
      },
      quotaBrokenEnv,
    )
    expect(missingQuotaResp.status).toBe(503)
    expect(missingQuotaResp.headers.get('Cache-Control')).toBe('no-store')
    expect(await missingQuotaResp.json()).toEqual(quotaBody)

    // PUT /v1/assets/:hash failing at checkAssetQuota -> 503 storage_limit_exceeded
    const putPrecheckResp = await callWorker(
      `https://sync.example.com/v1/assets/${hash}`,
      {
        method: 'PUT',
        headers: { ...authHeader('quota-err-user'), 'Content-Type': 'application/octet-stream' },
        body: bytes,
      },
      quotaBrokenEnv,
    )
    expect(putPrecheckResp.status).toBe(503)
    expect(putPrecheckResp.headers.get('Cache-Control')).toBe('no-store')
    expect(await putPrecheckResp.json()).toEqual(quotaBody)

    // PUT /v1/assets/:hash failing at recordAsset -> 503 and does NOT delete R2 object
    let r2DeleteCalled = false
    const recordFailEnv: Env = {
      ...TEST_ENV,
      USER_SYNC: {
        idFromName: TEST_ENV.USER_SYNC.idFromName.bind(TEST_ENV.USER_SYNC),
        get: () =>
          ({
            checkAssetQuota: async () => ({ status: 'allowed' }),
            recordAsset: async () => {
              throw quotaError
            },
            deleteAllData: async () => {
              throw quotaError
            },
          }) as unknown as ReturnType<Env['USER_SYNC']['get']>,
      } as unknown as Env['USER_SYNC'],
      ASSETS: {
        put: async () => null,
        delete: async () => {
          r2DeleteCalled = true
        },
        list: async () => ({ objects: [{ key: 'users/quota-err-user/assets/x' }], truncated: false }),
        get: async () => {
          throw new Error('R2 transient read error')
        },
      } as unknown as R2Bucket,
    }

    const putRecordFailResp = await callWorker(
      `https://sync.example.com/v1/assets/${hash}`,
      {
        method: 'PUT',
        headers: { ...authHeader('quota-err-user'), 'Content-Type': 'application/octet-stream' },
        body: bytes,
      },
      recordFailEnv,
    )
    expect(putRecordFailResp.status).toBe(503)
    expect(await putRecordFailResp.json()).toEqual(quotaBody)
    expect(r2DeleteCalled).toBe(false)

    // DELETE /v1/account when DO deleteAllData throws -> 503 JSON and R2 objects are NOT deleted first
    const deleteAccountFailResp = await callWorker(
      'https://sync.example.com/v1/account',
      {
        method: 'DELETE',
        headers: authHeader('quota-err-user'),
      },
      recordFailEnv,
    )
    expect(deleteAccountFailResp.status).toBe(503)
    expect(await deleteAccountFailResp.json()).toEqual(quotaBody)
    expect(r2DeleteCalled).toBe(false)

    // GET /v1/assets/:hash when R2 throws -> 500 internal_error
    const getR2FailResp = await callWorker(
      `https://sync.example.com/v1/assets/${hash}`,
      {
        method: 'GET',
        headers: authHeader('quota-err-user'),
      },
      recordFailEnv,
    )
    expect(getR2FailResp.status).toBe(500)
    expect(await getR2FailResp.json()).toEqual(internalBody)

    // PUT /v1/assets/:hash when R2 put throws -> 500 internal_error;
    // POST /v1/assets/missing when DO throws non-quota error -> 500 internal_error
    const genericStorageFailEnv: Env = {
      ...TEST_ENV,
      USER_SYNC: {
        idFromName: TEST_ENV.USER_SYNC.idFromName.bind(TEST_ENV.USER_SYNC),
        get: () =>
          ({
            missingAssets: async () => {
              throw new Error('SQLITE_BUSY')
            },
            checkAssetQuota: async () => ({ status: 'allowed' }),
          }) as unknown as ReturnType<Env['USER_SYNC']['get']>,
      } as unknown as Env['USER_SYNC'],
      ASSETS: {
        put: async () => {
          throw new Error('R2 put transient failure')
        },
      } as unknown as R2Bucket,
    }
    const missingGenericFailResp = await callWorker(
      'https://sync.example.com/v1/assets/missing',
      {
        method: 'POST',
        headers: { ...authHeader('quota-err-user'), 'Content-Type': 'application/json' },
        body: JSON.stringify({ hashes: [hash] }),
      },
      genericStorageFailEnv,
    )
    expect(missingGenericFailResp.status).toBe(500)
    expect(await missingGenericFailResp.json()).toEqual(internalBody)

    const putR2FailResp = await callWorker(
      `https://sync.example.com/v1/assets/${hash}`,
      {
        method: 'PUT',
        headers: { ...authHeader('quota-err-user'), 'Content-Type': 'application/octet-stream' },
        body: bytes,
      },
      genericStorageFailEnv,
    )
    expect(putR2FailResp.status).toBe(500)
    expect(await putR2FailResp.json()).toEqual(internalBody)

    // PUT /v1/assets/:hash when recordAsset returns quota_exceeded and R2 delete throws -> still 413
    const rollbackFailEnv: Env = {
      ...TEST_ENV,
      USER_SYNC: {
        idFromName: TEST_ENV.USER_SYNC.idFromName.bind(TEST_ENV.USER_SYNC),
        get: () =>
          ({
            checkAssetQuota: async () => ({ status: 'allowed' }),
            recordAsset: async () => ({ status: 'quota_exceeded' }),
          }) as unknown as ReturnType<Env['USER_SYNC']['get']>,
      } as unknown as Env['USER_SYNC'],
      ASSETS: {
        put: async () => null,
        delete: async () => {
          throw new Error('R2 delete failed')
        },
      } as unknown as R2Bucket,
    }
    const rollbackFailResp = await callWorker(
      `https://sync.example.com/v1/assets/${hash}`,
      {
        method: 'PUT',
        headers: { ...authHeader('quota-err-user'), 'Content-Type': 'application/octet-stream' },
        body: bytes,
      },
      rollbackFailEnv,
    )
    expect(rollbackFailResp.status).toBe(413)
    expect(await rollbackFailResp.json()).toEqual({ error: 'quota_exceeded' })
  })
})
