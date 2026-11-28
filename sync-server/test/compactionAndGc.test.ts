import { env, runDurableObjectAlarm, runInDurableObject, SELF } from 'cloudflare:test'
import { describe, expect, it } from 'vitest'
import type { Env } from '../src/types'

const TEST_ENV = env as unknown as Env
const DEV_SECRET = 'test-dev-secret'
const MS_PER_DAY = 86_400_000

function authHeader(userId: string): Record<string, string> {
  return { Authorization: `Bearer ${DEV_SECRET}:${userId}` }
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

async function gunzipJson<T>(bytes: Uint8Array): Promise<T> {
  let rawBytes = bytes
  if (bytes.byteLength >= 2 && bytes[0] === 0x1f && bytes[1] === 0x8b) {
    const stream = new Response(bytes).body!.pipeThrough(new DecompressionStream('gzip'))
    rawBytes = new Uint8Array(await new Response(stream).arrayBuffer())
  }
  return JSON.parse(new TextDecoder().decode(rawBytes)) as T
}

function makeDeckUpsertOp(id: string, name: string, ms: number, dev = 'aaaaaaaa') {
  return {
    opId: `op-deck-${id}-${ms}`,
    payload: {
      type: 'deckUpsert',
      id,
      name,
      color: '#4f46e5',
      settings: {
        newPerDay: 20,
        maxReviews: 200,
        learnSteps: '1m 10m',
        insertionOrder: 'sequential',
        relearnSteps: '10m',
        minimumInterval: 1,
        leechThreshold: 8,
        leechAction: 'tag',
        buryRelated: true,
        showTimer: false,
        desiredRetention: 0.9,
        maximumInterval: 36500,
        fsrsWeights: null,
      },
      createdAt: ms,
      hlc: `${String(ms).padStart(13, '0')}-0000-${dev}`,
    },
  }
}

function makeCardCreateOp(
  id: string,
  deckId: string,
  front: string,
  back: string,
  ms: number,
  dev = 'aaaaaaaa',
) {
  return {
    opId: `op-card-${id}-${ms}`,
    payload: {
      type: 'cardCreate',
      id,
      deckId,
      front,
      back,
      tags: ['v1'],
      createdAt: ms,
      initialDue: ms,
      hlc: `${String(ms).padStart(13, '0')}-0000-${dev}`,
    },
  }
}

function makeCardPatchOp(id: string, front: string, back: string, ms: number, dev = 'aaaaaaaa') {
  return {
    opId: `op-patch-${id}-${ms}`,
    payload: {
      type: 'cardContentPatch',
      id,
      front,
      back,
      tags: ['v2'],
      hlc: `${String(ms).padStart(13, '0')}-0000-${dev}`,
    },
  }
}

function makeCardReviewOp(
  cardId: string,
  deckId: string,
  reviewLogId: string,
  ms: number,
  dev = 'aaaaaaaa',
) {
  return {
    opId: `op-rev-${reviewLogId}`,
    payload: {
      type: 'cardReview',
      cardId,
      deckId,
      reviewedAt: ms,
      grade: 'good',
      scheduling: {
        kind: 'fsrs',
        stability: 3.5,
        difficulty: 5.0,
        reps: 1,
        lapses: 0,
        state: 2,
        step: 0,
        lastReview: ms,
        due: ms + MS_PER_DAY,
      },
      lastAgainAt: null,
      leechSuspended: false,
      leechTagAdded: false,
      reviewLogId,
      hlc: `${String(ms).padStart(13, '0')}-0000-${dev}`,
    },
  }
}

interface CompactedSnapshot {
  schemaVersion: number
  snapshotSeq: number
  maxHlc: string
  decks: Array<{ id: string; name: string }>
  cards: Array<{ id: string; deckId: string; front: string; back: string; tags: string[] }>
  reviewLogs: Array<[string, string, string, number, string]>
  tombstones: Array<{ id: string; kind: string; deletedAt: number; hlc: string }>
}

describe('Slice 3: WASM compaction, /v1/bootstrap, and retention pruning', () => {
  it('returns 404 from /v1/bootstrap before any snapshot exists and compacts via DO alarm with incremental R2 updates', async () => {
    const user = 'compact-alarm-user'

    // Unauthenticated /v1/bootstrap returns 401.
    const unauthBootstrap = await SELF.fetch('https://sync.example.com/v1/bootstrap')
    expect(unauthBootstrap.status).toBe(401)

    // Before compaction, /v1/bootstrap returns 404.
    const beforeBootstrap = await SELF.fetch('https://sync.example.com/v1/bootstrap', {
      headers: authHeader(user),
    })
    expect(beforeBootstrap.status).toBe(404)

    // Lower COMPACT_AFTER_OPS on this DO instance so 4 ops triggers the compaction alarm.
    const stub = TEST_ENV.USER_SYNC.get(TEST_ENV.USER_SYNC.idFromName(user))
    await runInDurableObject(stub, async (instance) => {
      const mutable = instance as unknown as { env: Env }
      mutable.env = { ...mutable.env, COMPACT_AFTER_OPS: '4', ENABLE_OP_PRUNING: 'false' }
    })

    const t0 = 1_700_000_000_000
    const ops1 = [
      makeDeckUpsertOp('d1', 'Japanese', t0),
      makeCardCreateOp('c1', 'd1', 'Front 1', 'Back 1', t0 + 1),
      makeCardPatchOp('c1', 'Front 1 edited', 'Back 1 edited', t0 + 2),
      makeCardReviewOp('c1', 'd1', 'rl-1', t0 + 3),
    ]

    const push1 = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 0, ops: ops1 }),
    })
    expect(push1.status).toBe(200)

    // Alarm should have been scheduled automatically and run WASM compaction.
    const ran1 = await runDurableObjectAlarm(stub)
    expect(ran1).toBe(true)

    // Verify immutable R2 snapshot at seq 4 exists.
    const r2Snap4 = await TEST_ENV.ASSETS.get(`users/${user}/snapshots/4.json.gz`)
    expect(r2Snap4).not.toBeNull()
    const snap4Bytes = new Uint8Array(await r2Snap4!.arrayBuffer())
    expect(snap4Bytes[0]).toBe(0x1f)
    expect(snap4Bytes[1]).toBe(0x8b)
    const snap4 = await gunzipJson<CompactedSnapshot>(snap4Bytes)
    expect(snap4.schemaVersion).toBe(1)
    expect(snap4.snapshotSeq).toBe(4)
    expect(snap4.decks.map((d) => [d.id, d.name])).toEqual([['d1', 'Japanese']])
    expect(snap4.cards).toHaveLength(1)
    expect(snap4.cards[0].front).toBe('Front 1 edited')
    expect(snap4.cards[0].back).toBe('Back 1 edited')
    expect(snap4.reviewLogs).toEqual([['rl-1', 'd1', 'c1', t0 + 3, 'good']])

    // Verify GET /v1/bootstrap returns exact headers and gzipped JSON stream.
    const bootResp = await SELF.fetch('https://sync.example.com/v1/bootstrap', {
      headers: authHeader(user),
    })
    expect(bootResp.status).toBe(200)
    expect(bootResp.headers.get('Content-Type')).toBe('application/json')
    expect(bootResp.headers.get('Content-Encoding')).toBe('gzip')
    expect(bootResp.headers.get('X-Rem-Snapshot-Seq')).toBe('4')
    expect(bootResp.headers.get('ETag')).toBeTruthy()
    const bootSnap = await gunzipJson<CompactedSnapshot>(new Uint8Array(await bootResp.arrayBuffer()))
    expect(bootSnap.snapshotSeq).toBe(4)
    expect(bootSnap.cards[0].front).toBe('Front 1 edited')

    // With ENABLE_OP_PRUNING="false", ops are NOT pruned and incremental sync from seq 0 still works.
    const pullNoPrune = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 0, ops: [] }),
    })
    const pullNoPruneBody = (await pullNoPrune.json()) as {
      bootstrapRequired?: boolean
      ops: Array<{ seq: number }>
      latestSeq: number
    }
    expect(pullNoPruneBody.bootstrapRequired).toBeUndefined()
    expect(pullNoPruneBody.latestSeq).toBe(4)
    expect(pullNoPruneBody.ops.map((o) => o.seq)).toEqual([1, 2, 3, 4])

    // Push 4 more ops to trigger a SECOND incremental compaction (loads seq 4 snapshot, produces seq 8, deletes seq 4).
    const ops2 = [
      makeCardCreateOp('c2', 'd1', 'Second card', 'Back 2', t0 + 10),
      makeCardCreateOp('c3', 'd1', 'Deleted card', 'Back 3', t0 + 11),
      {
        opId: 'op-del-c3',
        payload: {
          type: 'cardDelete',
          id: 'c3',
          hlc: `${String(t0 + 12).padStart(13, '0')}-0000-aaaaaaaa`,
        },
      },
      makeCardReviewOp('c2', 'd1', 'rl-2', t0 + 13),
    ]
    const push2 = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 4, ops: ops2 }),
    })
    expect(push2.status).toBe(200)

    const ran2 = await runDurableObjectAlarm(stub)
    expect(ran2).toBe(true)

    // Superseded seq 4 snapshot should have been best-effort deleted; seq 8 snapshot exists.
    expect(await TEST_ENV.ASSETS.get(`users/${user}/snapshots/4.json.gz`)).toBeNull()
    const r2Snap8 = await TEST_ENV.ASSETS.get(`users/${user}/snapshots/8.json.gz`)
    expect(r2Snap8).not.toBeNull()
    const snap8 = await gunzipJson<CompactedSnapshot>(new Uint8Array(await r2Snap8!.arrayBuffer()))
    expect(snap8.snapshotSeq).toBe(8)
    expect(snap8.cards.map((c) => c.id).sort()).toEqual(['c1', 'c2'])
    expect(snap8.tombstones.some((t) => t.id === 'c3' && t.kind === 'card')).toBe(true)
    expect(snap8.reviewLogs).toHaveLength(2)
  })

  it('prunes ops respecting RETAIN_TAIL_OPS and RETAIN_TAIL_DAYS, never regresses latestSeq, and acks stale offline pushes before bootstrapRequired', async () => {
    const user = 'prune-retention-user'
    const stub = TEST_ENV.USER_SYNC.get(TEST_ENV.USER_SYNC.idFromName(user))

    const nowMs = Date.now()
    const t0 = nowMs - 40 * MS_PER_DAY

    // Push 6 ops from device A.
    const ops = [
      makeDeckUpsertOp('d1', 'Retention Deck', t0),
      makeCardCreateOp('c1', 'd1', 'Card 1', 'B1', t0 + 1),
      makeCardCreateOp('c2', 'd1', 'Card 2', 'B2', t0 + 2),
      makeCardCreateOp('c3', 'd1', 'Card 3', 'B3', t0 + 3),
      makeCardCreateOp('c4', 'd1', 'Card 4', 'B4', t0 + 4),
      makeCardCreateOp('c5', 'd1', 'Card 5', 'B5', t0 + 5),
    ]
    const pushResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 0, ops }),
    })
    expect(pushResp.status).toBe(200)

    // Backdate ops 1..3 to 45 days ago, keep ops 4..6 as 5 days ago.
    await runInDurableObject(stub, async (instance, state) => {
      state.storage.sql.exec('UPDATE ops SET created_at = ? WHERE seq <= 3', nowMs - 45 * MS_PER_DAY)
      state.storage.sql.exec('UPDATE ops SET created_at = ? WHERE seq >= 4', nowMs - 5 * MS_PER_DAY)
      const mutable = instance as unknown as { env: Env }
      mutable.env = {
        ...mutable.env,
        ENABLE_OP_PRUNING: 'true',
        RETAIN_TAIL_OPS: '2',
        RETAIN_TAIL_DAYS: '30',
      }
      const res = await instance.runCompaction(nowMs)
      expect(res.compacted).toBe(true)
      expect(res.snapshotSeq).toBe(6)
      // Count limit would allow pruning up to seq 4 (6-2), but age limit (30d) protects seq 4..6 -> prune up to seq 3!
      expect(res.prunedUpToSeq).toBe(3)
      const remaining = state.storage.sql
        .exec<{ seq: number }>('SELECT seq FROM ops ORDER BY seq ASC')
        .toArray()
      expect(remaining.map((r) => r.seq)).toEqual([4, 5, 6])
    })

    // Caught-up client (sinceSeq=3 == minRetainedSeq=3) gets normal incremental ops 4..6 without bootstrapRequired.
    const caughtUp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 3, ops: [] }),
    })
    const caughtUpJson = (await caughtUp.json()) as {
      bootstrapRequired?: boolean
      ops: Array<{ seq: number }>
      latestSeq: number
    }
    expect(caughtUpJson.bootstrapRequired).toBeUndefined()
    expect(caughtUpJson.ops.map((o) => o.seq)).toEqual([4, 5, 6])
    expect(caughtUpJson.latestSeq).toBe(6)

    // Stale client (sinceSeq=1 < minRetainedSeq=3) pushes an offline edit:
    // server MUST still persist & ack the pushed op, advance latestSeq to 7, and return bootstrapRequired: true!
    const offlineOp = makeCardPatchOp('c1', 'Offline edit from stale device', 'B1', nowMs, 'cccccccc')
    const staleResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'cccccccc', sinceSeq: 1, ops: [offlineOp] }),
    })
    expect(staleResp.status).toBe(200)
    expect(await staleResp.json()).toEqual({
      latestSeq: 7,
      hasMore: false,
      ackedOpIds: [offlineOp.opId],
      ops: [],
      bootstrapRequired: true,
      snapshotSeq: 6,
    })

    // Verify another device can pull the stale client's persisted offline op at seq 7!
    const otherPull = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 6, ops: [] }),
    })
    const otherJson = (await otherPull.json()) as {
      latestSeq: number
      ops: Array<{ seq: number; opId: string }>
    }
    expect(otherJson.latestSeq).toBe(7)
    expect(otherJson.ops.map((o) => [o.seq, o.opId])).toEqual([[7, offlineOp.opId]])

    // Now prune ALL ops (RETAIN_TAIL_OPS=0, RETAIN_TAIL_DAYS=0) and verify latestSeq NEVER regresses!
    await runInDurableObject(stub, async (instance, state) => {
      const mutable = instance as unknown as { env: Env }
      mutable.env = {
        ...mutable.env,
        ENABLE_OP_PRUNING: 'true',
        RETAIN_TAIL_OPS: '0',
        RETAIN_TAIL_DAYS: '0',
      }
      const res = await instance.runCompaction(nowMs + 1000)
      expect(res.compacted).toBe(true)
      expect(res.snapshotSeq).toBe(7)
      expect(res.prunedUpToSeq).toBe(7)
      const count = state.storage.sql
        .exec<{ cnt: number }>('SELECT COUNT(*) AS cnt FROM ops')
        .one()
      expect(count.cnt).toBe(0)
    })

    // Even with an empty `ops` table, latestSeq stays 7 (never regresses to 0) and next insert gets seq 8!
    const afterFullPrune = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 7, ops: [] }),
    })
    expect(await afterFullPrune.json()).toEqual({
      latestSeq: 7,
      hasMore: false,
      ackedOpIds: [],
      ops: [],
    })

    const nextOp = makeCardPatchOp('c2', 'After full prune', 'B2', nowMs + 2000, 'bbbbbbbb')
    const afterInsert = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'bbbbbbbb', sinceSeq: 7, ops: [nextOp] }),
    })
    const afterInsertJson = (await afterInsert.json()) as { latestSeq: number }
    expect(afterInsertJson.latestSeq).toBe(8)
  })
})

describe('Slice 5: Grace-period R2 orphan asset GC', () => {
  it('deletes only unreferenced assets past grace period, protects snapshot + tail-op + grace-window assets, and updates asset_usage', async () => {
    const user = 'asset-gc-user'
    const stub = TEST_ENV.USER_SYNC.get(TEST_ENV.USER_SYNC.idFromName(user))

    const uploadAsset = async (tag: number, size = 200) => {
      const bytes = new Uint8Array(size).fill(tag)
      const hash = await sha256Hex(bytes)
      const resp = await SELF.fetch(`https://sync.example.com/v1/assets/${hash}`, {
        method: 'PUT',
        headers: { ...authHeader(user), 'Content-Type': 'image/png' },
        body: bytes,
      })
      expect(resp.status).toBe(200)
      return hash
    }

    const liveHash = await uploadAsset(1, 200)
    const orphanHash = await uploadAsset(2, 200)
    const graceHash = await uploadAsset(3, 200)
    const tailHash = await uploadAsset(4, 200)

    const nowMs = Date.now()
    const t0 = nowMs - 40 * MS_PER_DAY

    // Card 1 initially references orphanHash, then gets patched to reference liveHash.
    const ops = [
      makeDeckUpsertOp('d1', 'Media Deck', t0),
      makeCardCreateOp('c1', 'd1', `![old](asset:${orphanHash})`, 'back', t0 + 1),
      makeCardPatchOp('c1', `![live](asset:${liveHash})`, 'back', t0 + 2),
    ]
    const pushResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ deviceId: 'aaaaaaaa', sinceSeq: 0, ops }),
    })
    expect(pushResp.status).toBe(200)

    // Set asset uploaded_at timestamps:
    // - liveHash, orphanHash, tailHash: 40 days old (past 30-day grace period)
    // - graceHash: 10 days old (within 30-day grace period)
    // Also simulate an uncompacted tail op (seq=4, arrived concurrently after targetSeq=3)
    // by inserting seq 4 referencing tailHash AFTER reading targetSeq=3 via a custom R2 put hook!
    await runInDurableObject(stub, async (instance, state) => {
      state.storage.sql.exec(
        'UPDATE assets SET uploaded_at = ? WHERE hash IN (?, ?, ?)',
        nowMs - 40 * MS_PER_DAY,
        liveHash,
        orphanHash,
        tailHash,
      )
      state.storage.sql.exec(
        'UPDATE assets SET uploaded_at = ? WHERE hash = ?',
        nowMs - 10 * MS_PER_DAY,
        graceHash,
      )

      const mutable = instance as unknown as { env: Env }
      const origPut = mutable.env.ASSETS.put.bind(mutable.env.ASSETS)
      let tailInserted = false
      mutable.env = {
        ...mutable.env,
        ASSET_GC_GRACE_DAYS: '30',
        ASSETS: {
          ...mutable.env.ASSETS,
          get: mutable.env.ASSETS.get.bind(mutable.env.ASSETS),
          delete: mutable.env.ASSETS.delete.bind(mutable.env.ASSETS),
          list: mutable.env.ASSETS.list.bind(mutable.env.ASSETS),
          put: async (key: string, value: Parameters<R2Bucket['put']>[1], opts?: R2PutOptions) => {
            const res = await origPut(key, value, opts)
            if (!tailInserted && key.includes('/snapshots/')) {
              tailInserted = true
              // Simulate an op arriving concurrently while snapshot seq=3 was being written to R2!
              const tailOp = makeCardCreateOp('c-tail', 'd1', `![tail](asset:${tailHash})`, 'b', nowMs)
              state.storage.sql.exec(
                'INSERT INTO ops(op_id, device_id, payload, created_at) VALUES (?, ?, ?, ?)',
                tailOp.opId,
                'aaaaaaaa',
                JSON.stringify(tailOp.payload),
                nowMs,
              )
              state.storage.sql.exec(
                "INSERT OR REPLACE INTO sync_meta(key, value) VALUES ('latest_seq', '4')",
              )
            }
            return res
          },
        } as unknown as R2Bucket,
      }

      const result = await instance.runCompaction(nowMs)
      expect(result.compacted).toBe(true)
      expect(result.snapshotSeq).toBe(3)
      // ONLY orphanHash should be deleted!
      expect(result.deletedAssets).toEqual([orphanHash])

      // Verify asset_usage.total_bytes was decremented accurately (800 - 200 = 600).
      const usage = state.storage.sql
        .exec<{ total_bytes: number }>('SELECT total_bytes FROM asset_usage WHERE id = 1')
        .one()
      expect(usage.total_bytes).toBe(600)
    })

    // Verify R2 state: orphanHash gone, liveHash / graceHash / tailHash still present.
    expect(await TEST_ENV.ASSETS.get(`users/${user}/assets/${orphanHash}`)).toBeNull()
    expect(await TEST_ENV.ASSETS.get(`users/${user}/assets/${liveHash}`)).not.toBeNull()
    expect(await TEST_ENV.ASSETS.get(`users/${user}/assets/${graceHash}`)).not.toBeNull()
    expect(await TEST_ENV.ASSETS.get(`users/${user}/assets/${tailHash}`)).not.toBeNull()

    // Verify /v1/assets/missing reports orphanHash missing and the other 3 present.
    const missingResp = await SELF.fetch('https://sync.example.com/v1/assets/missing', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({ hashes: [liveHash, orphanHash, graceHash, tailHash] }),
    })
    expect(missingResp.status).toBe(200)
    expect(await missingResp.json()).toEqual({ missing: [orphanHash] })
  })

  it('aborts compaction cleanly and removes in-flight R2 snapshot if deleteAllData runs mid-compaction', async () => {
    const user = 'delete-mid-compaction-user'
    const stub = TEST_ENV.USER_SYNC.get(TEST_ENV.USER_SYNC.idFromName(user))
    const t0 = 1_700_000_000_000

    const pushResp = await SELF.fetch('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: { ...authHeader(user), 'Content-Type': 'application/json' },
      body: JSON.stringify({
        deviceId: 'aaaaaaaa',
        sinceSeq: 0,
        ops: [makeDeckUpsertOp('d1', 'Doomed Deck', t0)],
      }),
    })
    expect(pushResp.status).toBe(200)

    await runInDurableObject(stub, async (instance) => {
      const mutable = instance as unknown as { env: Env }
      const origPut = mutable.env.ASSETS.put.bind(mutable.env.ASSETS)
      const origDelete = mutable.env.ASSETS.delete.bind(mutable.env.ASSETS)
      mutable.env = {
        ...mutable.env,
        ASSETS: {
          ...mutable.env.ASSETS,
          get: mutable.env.ASSETS.get.bind(mutable.env.ASSETS),
          list: mutable.env.ASSETS.list.bind(mutable.env.ASSETS),
          delete: origDelete,
          put: async (key: string, value: Parameters<R2Bucket['put']>[1], opts?: R2PutOptions) => {
            const res = await origPut(key, value, opts)
            // Simulate concurrent DELETE /v1/account finishing while snapshot upload was in flight.
            await instance.deleteAllData()
            return res
          },
        } as unknown as R2Bucket,
      }

      const result = await instance.runCompaction(t0 + 1000)
      expect(result).toEqual({
        compacted: false,
        snapshotSeq: 0,
        prunedUpToSeq: 0,
        deletedAssets: [],
      })
    })

    // Zero zombie snapshot left behind in R2 after concurrent deleteAllData().
    expect(await TEST_ENV.ASSETS.get(`users/${user}/snapshots/1.json.gz`)).toBeNull()
  })
})
