import { SELF } from 'cloudflare:test'
import { describe, expect, it } from 'vitest'

const DEV_SECRET = 'test-dev-secret'
const JWT_SECRET = 'test-jwt-secret-key'

function authHeader(userId: string): Record<string, string> {
  return { Authorization: `Bearer ${DEV_SECRET}:${userId}` }
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

function toBase64Url(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i])
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

async function mintJwt(payload: Record<string, unknown>, secret = JWT_SECRET): Promise<string> {
  const enc = new TextEncoder()
  const headerB64 = toBase64Url(enc.encode(JSON.stringify({ alg: 'HS256', typ: 'JWT' })))
  const payloadB64 = toBase64Url(enc.encode(JSON.stringify(payload)))
  const key = await crypto.subtle.importKey(
    'raw',
    enc.encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const sig = new Uint8Array(
    await crypto.subtle.sign('HMAC', key, enc.encode(`${headerB64}.${payloadB64}`)),
  )
  return `${headerB64}.${payloadB64}.${toBase64Url(sig)}`
}

describe('rem-sync-server', () => {
  it('exposes an unauthenticated /health endpoint and rejects missing or bad tokens', async () => {
    const health = await SELF.fetch('https://sync.example.com/health')
    expect(health.status).toBe(200)
    expect(await health.json()).toEqual({ ok: true })

    const noAuth = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
    })
    expect(noAuth.status).toBe(401)

    const badToken = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { Authorization: 'Bearer wrong-secret:alice' },
      body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
    })
    expect(badToken.status).toBe(401)

    const badUserId = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { Authorization: `Bearer ${DEV_SECRET}:../evil` },
      body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
    })
    expect(badUserId.status).toBe(401)
  })

  it('verifies HS256 JWTs and rejects expired or tampered tokens', async () => {
    const nowSec = Math.floor(Date.now() / 1000)
    const validJwt = await mintJwt({ sub: 'jwt-user-1', exp: nowSec + 300 })
    const ok = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${validJwt}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
    })
    expect(ok.status).toBe(200)
    expect(await ok.json()).toEqual({
      latestSeq: 0,
      hasMore: false,
      ackedOpIds: [],
      ops: [],
    })

    const expiredJwt = await mintJwt({ sub: 'jwt-user-1', exp: nowSec - 10 })
    const expired = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { Authorization: `Bearer ${expiredJwt}` },
      body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
    })
    expect(expired.status).toBe(401)

    const wrongSigJwt = await mintJwt({ sub: 'jwt-user-1', exp: nowSec + 300 }, 'other-secret')
    const tampered = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { Authorization: `Bearer ${wrongSigJwt}` },
      body: JSON.stringify({ deviceId: 'dev1', sinceSeq: 0, ops: [] }),
    })
    expect(tampered.status).toBe(401)
  })

  it('assigns strict seq numbers, deduplicates opId retries, and paginates with hasMore', async () => {
    const user = 'sync-user-seq'
    const op1 = {
      opId: 'op-1',
      payload: { type: 'deckUpsert', id: 'd1', name: 'Rust', hlc: '0000000001000-0000-aaaaaaaa' },
    }
    const op2 = {
      opId: 'op-2',
      payload: { type: 'cardCreate', id: 'c1', deckId: 'd1', hlc: '0000000001001-0000-aaaaaaaa' },
    }
    const op3 = {
      opId: 'op-3',
      payload: { type: 'cardDelete', id: 'c1', hlc: '0000000001002-0000-aaaaaaaa' },
    }

    // Push op1 & op2 from device A.
    const push1 = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 0, ops: [op1, op2] }),
    })
    expect(push1.status).toBe(200)
    const body1 = (await push1.json()) as {
      latestSeq: number
      hasMore: boolean
      ackedOpIds: string[]
      ops: Array<{ seq: number; opId: string; deviceId: string }>
    }
    expect(body1.latestSeq).toBe(2)
    expect(body1.hasMore).toBe(false)
    expect(body1.ackedOpIds).toEqual(['op-1', 'op-2'])
    expect(body1.ops.map((o) => [o.seq, o.opId])).toEqual([
      [1, 'op-1'],
      [2, 'op-2'],
    ])

    // Retry op2 alongside new op3 -> op2 deduplicated, op3 gets seq 3.
    const push2 = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 2, ops: [op2, op3] }),
    })
    expect(push2.status).toBe(200)
    const body2 = (await push2.json()) as {
      latestSeq: number
      ackedOpIds: string[]
      ops: Array<{ seq: number; opId: string }>
    }
    expect(body2.latestSeq).toBe(3)
    expect(body2.ackedOpIds).toEqual(['op-2', 'op-3'])
    expect(body2.ops.map((o) => [o.seq, o.opId])).toEqual([[3, 'op-3']])

    // Device B pulls with limit=2 -> gets seq 1..2 with hasMore=true, then seq 3 with hasMore=false.
    const pull1 = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 0, limit: 2, ops: [] }),
    })
    const p1 = (await pull1.json()) as {
      latestSeq: number
      hasMore: boolean
      ops: Array<{ seq: number; opId: string }>
    }
    expect(p1.latestSeq).toBe(3)
    expect(p1.hasMore).toBe(true)
    expect(p1.ops.map((o) => o.seq)).toEqual([1, 2])

    const pull2 = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 2, limit: 2, ops: [] }),
    })
    const p2 = (await pull2.json()) as {
      latestSeq: number
      hasMore: boolean
      ops: Array<{ seq: number; opId: string }>
    }
    expect(p2.latestSeq).toBe(3)
    expect(p2.hasMore).toBe(false)
    expect(p2.ops.map((o) => o.seq)).toEqual([3])
  })

  it('isolates ops and R2 assets strictly between users', async () => {
    const payload = new TextEncoder().encode('alice-secret-image')
    const hash = await sha256Hex(payload)

    // Alice pushes an op and uploads an asset.
    const syncAlice = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader('alice'), 'Content-Type': 'application/json' },
      body: JSON.stringify({
        deviceId: 'dev-a',
        sinceSeq: 0,
        ops: [
          {
            opId: 'alice-op-1',
            payload: { type: 'deckUpsert', id: 'd-alice', hlc: '0000000001000-0000-aaaaaaaa' },
          },
        ],
      }),
    })
    expect(syncAlice.status).toBe(200)

    const putAlice = await SELF.fetch(`https://sync.example.com/v1/assets/${hash}`, {
      method: 'PUT',
      headers: { ...authHeader('alice'), 'Content-Type': 'image/png' },
      body: payload,
    })
    expect(putAlice.status).toBe(200)

    // Bob sees zero ops and cannot access Alice's asset.
    const syncBob = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader('bob'), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'dev-b', sinceSeq: 0, ops: [] }),
    })
    expect(await syncBob.json()).toEqual({
      latestSeq: 0,
      hasMore: false,
      ackedOpIds: [],
      ops: [],
    })

    const bobAsset = await SELF.fetch(`https://sync.example.com/v1/assets/${hash}`, {
      method: 'GET',
      headers: authHeader('bob'),
    })
    expect(bobAsset.status).toBe(404)
  })

  it('verifies asset SHA-256, checks missing via DO, streams from R2, and enforces size/quota limits', async () => {
    const user = 'asset-user'
    const bytes1 = new Uint8Array(900).fill(1)
    const hash1 = await sha256Hex(bytes1)
    const bytes2 = new Uint8Array(900).fill(2)
    const hash2 = await sha256Hex(bytes2)
    const bytes3 = new Uint8Array(900).fill(3)
    const hash3 = await sha256Hex(bytes3)
    const oversized = new Uint8Array(1500).fill(9)
    const oversizedHash = await sha256Hex(oversized)

    // Missing check reports both missing before upload.
    const missingBefore = await SELF.fetch('https://sync.example.com/v1/assets/missing', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ hashes: [hash1, hash2, hash1] }),
    })
    expect(missingBefore.status).toBe(200)
    expect(await missingBefore.json()).toEqual({ missing: [hash1, hash2] })

    // Hash mismatch rejected with 400.
    const mismatch = await SELF.fetch(`https://sync.example.com/v1/assets/${hash1}`, {
      method: 'PUT',
      headers: { ...authHeader(user), 'Content-Type': 'image/png' },
      body: bytes2,
    })
    expect(mismatch.status).toBe(400)

    // Single asset exceeding MAX_ASSET_BYTES (1024 in test config) rejected with 413.
    const tooBig = await SELF.fetch(`https://sync.example.com/v1/assets/${oversizedHash}`, {
      method: 'PUT',
      headers: { ...authHeader(user), 'Content-Type': 'image/png' },
      body: oversized,
    })
    expect(tooBig.status).toBe(413)

    // Valid upload succeeds and is idempotent on re-upload.
    const put1 = await SELF.fetch(`https://sync.example.com/v1/assets/${hash1}`, {
      method: 'PUT',
      headers: { ...authHeader(user), 'Content-Type': 'image/png' },
      body: bytes1,
    })
    expect(put1.status).toBe(200)

    const put1Again = await SELF.fetch(`https://sync.example.com/v1/assets/${hash1}`, {
      method: 'PUT',
      headers: { ...authHeader(user), 'Content-Type': 'image/png' },
      body: bytes1,
    })
    expect(put1Again.status).toBe(200)

    // Missing check now reports only hash2.
    const missingAfter = await SELF.fetch('https://sync.example.com/v1/assets/missing', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ hashes: [hash1, hash2] }),
    })
    expect(await missingAfter.json()).toEqual({ missing: [hash2] })

    // Download streams exact bytes with private immutable cache header.
    const get1 = await SELF.fetch(`https://sync.example.com/v1/assets/${hash1}`, {
      method: 'GET',
      headers: authHeader(user),
    })
    expect(get1.status).toBe(200)
    expect(get1.headers.get('Content-Type')).toBe('image/png')
    expect(get1.headers.get('Cache-Control')).toBe('private, max-age=31536000, immutable')
    expect(new Uint8Array(await get1.arrayBuffer())).toEqual(bytes1)

    // Upload second 900B asset (total 1800 <= 2048 quota) -> ok; third 900B asset (2700 > 2048) -> 413 quota_exceeded.
    const put2 = await SELF.fetch(`https://sync.example.com/v1/assets/${hash2}`, {
      method: 'PUT',
      headers: { ...authHeader(user), 'Content-Type': 'image/gif' },
      body: bytes2,
    })
    expect(put2.status).toBe(200)

    const put3 = await SELF.fetch(`https://sync.example.com/v1/assets/${hash3}`, {
      method: 'PUT',
      headers: { ...authHeader(user), 'Content-Type': 'image/png' },
      body: bytes3,
    })
    expect(put3.status).toBe(413)
    expect(await put3.json()).toEqual({ error: 'quota_exceeded' })
  })
})
