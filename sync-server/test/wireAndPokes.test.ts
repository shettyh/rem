import { env, SELF } from 'cloudflare:test'
import { describe, expect, it } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import worker from '../src/index'
import type { Env } from '../src/types'

const TEST_ENV = env as unknown as Env
const DEV_SECRET = 'test-dev-secret'
const JWT_SECRET = 'test-jwt-secret-key'
const TEXT_ENCODER = new TextEncoder()

function authHeader(userId: string): Record<string, string> {
  return { Authorization: `Bearer ${DEV_SECRET}:${userId}` }
}

function toBase64Url(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i])
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

async function mintJwt(payload: Record<string, unknown>, secret = JWT_SECRET): Promise<string> {
  const headerB64 = toBase64Url(TEXT_ENCODER.encode(JSON.stringify({ alg: 'HS256', typ: 'JWT' })))
  const payloadB64 = toBase64Url(TEXT_ENCODER.encode(JSON.stringify(payload)))
  const key = await crypto.subtle.importKey(
    'raw',
    TEXT_ENCODER.encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const sig = new Uint8Array(
    await crypto.subtle.sign('HMAC', key, TEXT_ENCODER.encode(`${headerB64}.${payloadB64}`)),
  )
  return `${headerB64}.${payloadB64}.${toBase64Url(sig)}`
}

async function gzipBytes(input: Uint8Array): Promise<Uint8Array> {
  const cs = new CompressionStream('gzip')
  const writer = cs.writable.getWriter()
  const writePromise = writer.write(input).then(() => writer.close())
  const chunks: Uint8Array[] = []
  let total = 0
  const reader = cs.readable.getReader()
  while (true) {
    const { value, done } = await reader.read()
    if (done) break
    if (value) {
      chunks.push(value)
      total += value.byteLength
    }
  }
  await writePromise
  const out = new Uint8Array(total)
  let offset = 0
  for (const c of chunks) {
    out.set(c, offset)
    offset += c.byteLength
  }
  return out
}

async function applyAuthMigrations(): Promise<void> {
  for (const raw of [
    migrationSql,
    entitlementsMigrationSql,
    earlyAccessMigrationSql,
    trialInvitesBillingMigrationSql,
  ]) {
    const statements = raw
      .split(';')
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
    for (const sql of statements) {
      await TEST_ENV.DB!.prepare(sql).run()
    }
  }
}

function waitForMessage(ws: WebSocket, timeoutMs = 1500): Promise<string> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cleanup()
      reject(new Error('timed out waiting for WebSocket message'))
    }, timeoutMs)
    const onMessage = (ev: MessageEvent) => {
      cleanup()
      resolve(typeof ev.data === 'string' ? ev.data : new TextDecoder().decode(ev.data as ArrayBuffer))
    }
    const onError = () => {
      cleanup()
      reject(new Error('WebSocket error while waiting for message'))
    }
    const cleanup = () => {
      clearTimeout(timer)
      ws.removeEventListener('message', onMessage)
      ws.removeEventListener('error', onError)
    }
    ws.addEventListener('message', onMessage)
    ws.addEventListener('error', onError)
  })
}

function expectNoMessage(ws: WebSocket, windowMs = 150): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      ws.removeEventListener('message', onMessage)
      resolve()
    }, windowMs)
    const onMessage = (ev: MessageEvent) => {
      clearTimeout(timer)
      ws.removeEventListener('message', onMessage)
      reject(new Error(`unexpected WebSocket message received: ${String(ev.data)}`))
    }
    ws.addEventListener('message', onMessage)
  })
}

function waitForClose(ws: WebSocket, timeoutMs = 1500): Promise<{ code: number; reason: string }> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      ws.removeEventListener('close', onClose)
      reject(new Error('timed out waiting for WebSocket close'))
    }, timeoutMs)
    const onClose = (ev: CloseEvent) => {
      clearTimeout(timer)
      ws.removeEventListener('close', onClose)
      resolve({ code: ev.code, reason: ev.reason })
    }
    ws.addEventListener('close', onClose)
  })
}

describe('Slice 1: echo suppression & gzip request support', () => {
  it('suppresses sender echoes while keeping interleaved cursor pagination and latestSeq sound', async () => {
    const user = 'echo-suppression-user'
    const pushOp = async (deviceId: string, opId: string, ms: number) => {
      const hlc = `${String(ms).padStart(13, '0')}-0000-${deviceId}`
      const resp = await SELF.fetch('https://sync.example.com/v1/sync', {
        method: 'POST',
        headers: { ...authHeader(user), 'Content-Type': 'application/json' },
        body: JSON.stringify({
          deviceId,
          sinceSeq: 0,
          ops: [{ opId, payload: { type: 'deckUpsert', id: opId, name: opId, hlc } }],
        }),
      })
      expect(resp.status).toBe(200)
      return (await resp.json()) as {
        latestSeq: number
        hasMore: boolean
        ackedOpIds: string[]
        ops: Array<{ seq: number; opId: string; deviceId: string }>
      }
    }

    // Interleave: seq1=A, seq2=B, seq3=A, seq4=B, seq5=B
    const r1 = await pushOp('aaaaaaaa', 'op-a1', 1000)
    expect(r1.latestSeq).toBe(1)
    expect(r1.ops).toEqual([])

    const r2 = await pushOp('bbbbbbbb', 'op-b1', 1001)
    expect(r2.latestSeq).toBe(2)
    expect(r2.ops.map((o) => [o.seq, o.opId])).toEqual([[1, 'op-a1']])

    const r3 = await pushOp('aaaaaaaa', 'op-a2', 1002)
    expect(r3.latestSeq).toBe(3)
    expect(r3.ops.map((o) => [o.seq, o.opId])).toEqual([[2, 'op-b1']])

    await pushOp('bbbbbbbb', 'op-b2', 1003)
    await pushOp('bbbbbbbb', 'op-b3', 1004)

    // Device A paginates with limit=1 starting from sinceSeq=0 -> sees only B's ops (2, 4, 5)
    // without skipping any remote ops or stalling hasMore.
    const seen: Array<[number, string]> = []
    let cursor = 0
    while (true) {
      const resp = await SELF.fetch('https://sync.example.com/v1/sync', {
        method: 'POST',
        headers: { ...authHeader(user), 'Content-Type': 'application/json' },
        body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: cursor, limit: 1, ops: [] }),
      })
      expect(resp.status).toBe(200)
      const page = (await resp.json()) as {
        latestSeq: number
        hasMore: boolean
        ops: Array<{ seq: number; opId: string; deviceId: string }>
      }
      expect(page.latestSeq).toBe(5)
      for (const op of page.ops) {
        expect(op.deviceId).not.toBe('aaaaaaaa')
        seen.push([op.seq, op.opId])
        cursor = op.seq
      }
      if (!page.hasMore) {
        cursor = Math.max(cursor, page.latestSeq)
        break
      }
    }
    expect(seen).toEqual([
      [2, 'op-b1'],
      [4, 'op-b2'],
      [5, 'op-b3'],
    ])
    expect(cursor).toBe(5)
  })

  it('transparently accepts gzip, x-gzip, and auto-decompressed bodies while rejecting corrupt gzip, unsupported encodings, and zip bombs', async () => {
    const user = 'gzip-sync-user'
    const payload = JSON.stringify({
      deviceId: 'aaaaaaaa',
      sinceSeq: 0,
      ops: [
        {
          opId: 'gz-op-1',
          payload: {
            type: 'deckUpsert',
            id: 'd1',
            name: 'Compressed Deck',
            hlc: '0000000002000-0000-aaaaaaaa',
          },
        },
      ],
    })

    // 1. Real gzip body with Content-Encoding: gzip
    const gzBody = await gzipBytes(TEXT_ENCODER.encode(payload))
    const gzResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'gzip',
      },
      body: gzBody,
    })
    expect(gzResp.status).toBe(200)
    expect(await gzResp.json()).toEqual({
      latestSeq: 1,
      hasMore: false,
      ackedOpIds: ['gz-op-1'],
      ops: [],
    })

    // 2. x-gzip alias
    const xgzPayload = JSON.stringify({
      deviceId: 'bbbbbbbb',
      sinceSeq: 0,
      ops: [],
    })
    const xgzResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'x-gzip',
      },
      body: await gzipBytes(TEXT_ENCODER.encode(xgzPayload)),
    })
    expect(xgzResp.status).toBe(200)
    const xgzJson = (await xgzResp.json()) as { ops: Array<{ opId: string }> }
    expect(xgzJson.ops.map((o) => o.opId)).toEqual(['gz-op-1'])

    // 3. Simulated runtime auto-decompressed body (Content-Encoding: gzip header, plain JSON bytes)
    const autoDecompressedResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'gzip',
      },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 0, ops: [] }),
    })
    expect(autoDecompressedResp.status).toBe(200)
    const autoJson = (await autoDecompressedResp.json()) as { latestSeq: number }
    expect(autoJson.latestSeq).toBe(1)

    // 4. Unsupported Content-Encoding -> 415
    const brResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'br',
      },
      body: '{}',
    })
    expect(brResp.status).toBe(415)
    expect(await brResp.json()).toMatchObject({ error: 'unsupported_content_encoding' })
  })

  it('rejects corrupt gzip payloads with 400 invalid_gzip', async () => {
    const user = 'gzip-corrupt-user'
    // Short truncated header (<18 bytes)
    const truncatedGzip = new Uint8Array([0x1f, 0x8b, 0x08, 0x00, 0xde, 0xad, 0xbe, 0xef])
    const truncatedResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'gzip',
      },
      body: truncatedGzip,
    })
    expect(truncatedResp.status).toBe(400)
    expect(await truncatedResp.json()).toMatchObject({ error: 'invalid_gzip' })

    // >=18 bytes with invalid RFC 1952 compression method / reserved FLG bits
    const invalidHeaderGzip = new Uint8Array(32)
    invalidHeaderGzip.set([0x1f, 0x8b, 0x09, 0xe0, 0, 0, 0, 0, 0, 0xff])
    const invalidHeaderResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'gzip',
      },
      body: invalidHeaderGzip,
    })
    expect(invalidHeaderResp.status).toBe(400)
    expect(await invalidHeaderResp.json()).toMatchObject({ error: 'invalid_gzip' })

    // Non-gzip non-JSON bytes with Content-Encoding: gzip -> 400 invalid_json
    const nonGzipGarbage = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'gzip',
      },
      body: 'not-gzip-and-not-json',
    })
    expect(nonGzipGarbage.status).toBe(400)
    expect(await nonGzipGarbage.json()).toMatchObject({ error: 'invalid_json' })
  })

  it('rejects gzip zip bombs exceeding 8 MiB decompressed cap with 413 payload_too_large', async () => {
    const user = 'gzip-bomb-user'
    const bombRaw = new Uint8Array(9 * 1024 * 1024)
    const bombGz = await gzipBytes(bombRaw)
    expect(bombGz.byteLength).toBeLessThan(64 * 1024)
    const bombResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        ...authHeader(user),
        'Content-Type': 'application/json',
        'Content-Encoding': 'gzip',
      },
      body: bombGz,
    })
    expect(bombResp.status).toBe(413)
    expect(await bombResp.json()).toMatchObject({ error: 'payload_too_large' })
  })
})

describe('Slice 4: Hibernatable WebSocket pokes', () => {
  it('enforces header-only auth, entitlements, Upgrade header, and X-Rem-Device-Id on GET /v1/sync/ws', async () => {
    await applyAuthMigrations()
    const nowSec = Math.floor(Date.now() / 1000)

    // Create a D1 user so JWT account check passes when testing entitlement gating.
    const userId = 'ws-auth-user'
    await TEST_ENV.DB!.prepare(
      'INSERT OR REPLACE INTO users(id, email, created_at) VALUES (?1, ?2, ?3)',
    )
      .bind(userId, 'ws-auth@example.com', Date.now())
      .run()
    const validJwt = await mintJwt({ sub: userId, exp: nowSec + 300 })

    // Query-string token without Authorization header must be rejected with 401.
    const queryTokenResp = await SELF.fetch(
      `https://sync.example.com/v1/sync/ws?token=${encodeURIComponent(validJwt)}`,
      {
        headers: {
          Upgrade: 'websocket',
          'X-Rem-Device-Id': 'aaaaaaaa',
        },
      },
    )
    expect(queryTokenResp.status).toBe(401)

    // Missing Upgrade header -> 426.
    const noUpgradeResp = await SELF.fetch('https://sync.example.com/v1/sync/ws', {
      headers: {
        Authorization: `Bearer ${validJwt}`,
        'X-Rem-Device-Id': 'aaaaaaaa',
      },
    })
    expect(noUpgradeResp.status).toBe(426)

    // Missing or invalid X-Rem-Device-Id -> 400.
    const noDeviceResp = await SELF.fetch('https://sync.example.com/v1/sync/ws', {
      headers: {
        Authorization: `Bearer ${validJwt}`,
        Upgrade: 'websocket',
      },
    })
    expect(noDeviceResp.status).toBe(400)

    const longDeviceResp = await SELF.fetch('https://sync.example.com/v1/sync/ws', {
      headers: {
        Authorization: `Bearer ${validJwt}`,
        Upgrade: 'websocket',
        'X-Rem-Device-Id': 'x'.repeat(65),
      },
    })
    expect(longDeviceResp.status).toBe(400)

    // Unpaid user when REQUIRE_SUBSCRIPTION="true" -> 402.
    const gatedEnv: Env = {
      ...TEST_ENV,
      REQUIRE_SUBSCRIPTION: 'true',
      UPGRADE_URL: 'https://rem-sync.shettyh.fyi/premium',
    }
    const unpaidWsResp = await worker.fetch(
      new Request('https://sync.example.com/v1/sync/ws', {
        headers: {
          Authorization: `Bearer ${validJwt}`,
          Upgrade: 'websocket',
          'X-Rem-Device-Id': 'aaaaaaaa',
        },
      }),
      gatedEnv,
    )
    expect(unpaidWsResp.status).toBe(402)
  })

  it('auto-responds ping/pong, broadcasts pokes to other devices only, skips retries, and closes sockets with 4001 on account deletion', async () => {
    const user = 'ws-poke-user'

    // Connect two devices for the same user.
    const respA = await SELF.fetch('https://sync.example.com/v1/sync/ws', {
      headers: {
        ...authHeader(user),
        Upgrade: 'websocket',
        'X-Rem-Device-Id': 'aaaaaaaa',
      },
    })
    expect(respA.status).toBe(101)
    const wsA = respA.webSocket!
    wsA.accept()

    const respB = await SELF.fetch('https://sync.example.com/v1/sync/ws', {
      headers: {
        ...authHeader(user),
        Upgrade: 'websocket',
        'X-Rem-Device-Id': 'bbbbbbbb',
      },
    })
    expect(respB.status).toBe(101)
    const wsB = respB.webSocket!
    wsB.accept()

    // 1. Keepalive ping -> auto-responded pong.
    const pongPromise = waitForMessage(wsA)
    wsA.send('ping')
    expect(await pongPromise).toBe('pong')

    // 2. Device A pushes a new op -> Device B gets poke, Device A gets NO echo poke.
    const pokeForB = waitForMessage(wsB)
    const noEchoForA = expectNoMessage(wsA)

    const op1 = {
      opId: 'poke-op-1',
      payload: { type: 'deckUpsert', id: 'd1', name: 'Poke Deck', hlc: '0000000003000-0000-aaaaaaaa' },
    }
    const syncPush = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 0, ops: [op1] }),
    })
    expect(syncPush.status).toBe(200)

    expect(JSON.parse(await pokeForB)).toEqual({ type: 'poke', latestSeq: 1 })
    await noEchoForA

    // 3. Device A retries the same opId (0 newly inserted ops) -> no duplicate poke sent.
    const noRetryPokeForB = expectNoMessage(wsB)
    const retryPush = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 1, ops: [op1] }),
    })
    expect(retryPush.status).toBe(200)
    await noRetryPokeForB

    // 4. Plant both an R2 asset and an R2 snapshot for the user, then DELETE /v1/account:
    //    both R2 objects must be deleted AND active WS sockets closed with code 4001.
    const assetKey = `users/${user}/assets/${'a'.repeat(64)}`
    const snapshotKey = `users/${user}/snapshots/1.json.gz`
    await TEST_ENV.ASSETS.put(assetKey, new Uint8Array([1, 2, 3]))
    await TEST_ENV.ASSETS.put(snapshotKey, new Uint8Array([0x1f, 0x8b, 0x08]))

    const closeA = waitForClose(wsA)
    const closeB = waitForClose(wsB)

    const delResp = await SELF.fetch('https://sync.example.com/v1/account', {
      method: 'DELETE',
      headers: authHeader(user),
    })
    expect(delResp.status).toBe(200)

    const [evA, evB] = await Promise.all([closeA, closeB])
    expect(evA.code).toBe(4001)
    expect(evB.code).toBe(4001)

    expect(await TEST_ENV.ASSETS.get(assetKey)).toBeNull()
    expect(await TEST_ENV.ASSETS.get(snapshotKey)).toBeNull()
  })
})
