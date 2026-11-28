import { DurableObject } from 'cloudflare:workers'
import { createSnapshotReducer, extractAssetRefsFromText } from './reducer'
import type {
  BootstrapInfo,
  Env,
  PokeMessage,
  QuotaCheckResult,
  RemoteSyncOp,
  SyncRequest,
  SyncResponse,
  SyncRpcResult,
} from './types'

const MAX_SYNC_LIMIT = 500
const DEFAULT_SYNC_LIMIT = 500
const MAX_PUSH_BATCH = 500
const MAX_OP_PAYLOAD_BYTES = 256 * 1024
const MAX_BATCH_PAYLOAD_BYTES = 2 * 1024 * 1024
const MAX_DEVICE_ID_LENGTH = 64
const DEFAULT_COMPACT_AFTER_OPS = 5000
const DEFAULT_RETAIN_TAIL_OPS = 2000
const DEFAULT_RETAIN_TAIL_DAYS = 30
const DEFAULT_ASSET_GC_GRACE_DAYS = 30
const COMPACTION_CHUNK_SIZE = 500
const COMPACTION_ALARM_DELAY_MS = 1000
const MS_PER_DAY = 86_400_000
const HLC_RE = /^\d{13}-\d{4}-[a-z0-9]{8}$/
const TEXT_ENCODER = new TextEncoder()
const TEXT_DECODER = new TextDecoder()

export const WS_CLOSE_ACCOUNT_DELETED = 4001

interface WsAttachment {
  deviceId: string
}

function parsePositiveInt(raw: string | undefined, fallback: number): number {
  if (!raw) return fallback
  const parsed = Number.parseInt(raw, 10)
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : fallback
}

function parseNonNegativeInt(raw: string | undefined, fallback: number): number {
  if (raw === undefined || raw === '') return fallback
  const parsed = Number.parseInt(raw, 10)
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : fallback
}

function concatUint8Arrays(chunks: Uint8Array[], totalBytes: number): Uint8Array {
  if (chunks.length === 0) return new Uint8Array(0)
  if (chunks.length === 1) return chunks[0]
  const out = new Uint8Array(totalBytes)
  let offset = 0
  for (const chunk of chunks) {
    out.set(chunk, offset)
    offset += chunk.byteLength
  }
  return out
}

async function readAllBytes(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const reader = stream.getReader()
  const chunks: Uint8Array[] = []
  let totalBytes = 0
  try {
    while (true) {
      const { value, done } = await reader.read()
      if (done) break
      if (!value || value.byteLength === 0) continue
      chunks.push(value)
      totalBytes += value.byteLength
    }
    return concatUint8Arrays(chunks, totalBytes)
  } finally {
    reader.releaseLock()
  }
}

async function gzipString(text: string): Promise<Uint8Array> {
  const input = TEXT_ENCODER.encode(text)
  const compressed = new Response(input).body!.pipeThrough(new CompressionStream('gzip'))
  return readAllBytes(compressed)
}

async function gunzipBytesToString(bytes: Uint8Array): Promise<string> {
  if (bytes.byteLength >= 2 && bytes[0] === 0x1f && bytes[1] === 0x8b) {
    const decompressed = new Response(bytes).body!.pipeThrough(new DecompressionStream('gzip'))
    const plain = await readAllBytes(decompressed)
    return TEXT_DECODER.decode(plain)
  }
  return TEXT_DECODER.decode(bytes)
}

function snapshotR2Key(userId: string, snapshotSeq: number): string {
  return `users/${userId}/snapshots/${snapshotSeq}.json.gz`
}

function assetR2Key(userId: string, hash: string): string {
  return `users/${userId}/assets/${hash}`
}

function validationFailure(error: string): SyncRpcResult {
  return { status: 'invalid', error }
}

export class UserSyncDO extends DurableObject<Env> {
  private schemaReady = false
  private cachedUserId: string | null = null
  private generation = 0

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env)
    // Auto-respond to WebSocket keepalive "ping" frames with "pong" without waking the DO.
    this.ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('ping', 'pong'))
  }

  private ensureSchema(): void {
    if (this.schemaReady) return
    this.ctx.storage.transactionSync(() => {
      this.ctx.storage.sql.exec(`
        CREATE TABLE IF NOT EXISTS ops (
          seq INTEGER PRIMARY KEY AUTOINCREMENT,
          op_id TEXT UNIQUE NOT NULL,
          device_id TEXT NOT NULL,
          payload TEXT NOT NULL,
          created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS assets (
          hash TEXT PRIMARY KEY,
          size_bytes INTEGER NOT NULL,
          mime TEXT NOT NULL,
          uploaded_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS asset_usage (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          total_bytes INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_meta (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
      `)
    })
    this.schemaReady = true
  }

  private getMeta(key: string): string | null {
    const rows = this.ctx.storage.sql
      .exec<{ value: string }>('SELECT value FROM sync_meta WHERE key = ?', key)
      .toArray()
    return rows.length > 0 ? rows[0].value : null
  }

  private getMetaInt(key: string, fallback = 0): number {
    const raw = this.getMeta(key)
    if (raw === null) return fallback
    const parsed = Number.parseInt(raw, 10)
    return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : fallback
  }

  private setMeta(key: string, value: string): void {
    this.ctx.storage.sql.exec(
      'INSERT OR REPLACE INTO sync_meta(key, value) VALUES (?, ?)',
      key,
      value,
    )
  }

  private persistUserId(userId?: string): void {
    if (!userId || userId.length === 0) return
    if (this.cachedUserId === userId) return
    this.setMeta('user_id', userId)
    this.cachedUserId = userId
  }

  private readLatestSeq(): number {
    const metaRaw = this.getMeta('latest_seq')
    if (metaRaw !== null) {
      const parsed = Number.parseInt(metaRaw, 10)
      if (Number.isSafeInteger(parsed) && parsed >= 0) {
        return parsed
      }
    }
    // Lazy one-time backfill for DOs created before sync_meta existed.
    const row = this.ctx.storage.sql
      .exec<{ latest_seq: number }>('SELECT COALESCE(MAX(seq), 0) AS latest_seq FROM ops')
      .one()
    this.setMeta('latest_seq', String(row.latest_seq))
    return row.latest_seq
  }

  private readTotalAssetBytes(): number {
    const usageRow = this.ctx.storage.sql
      .exec<{ total_bytes: number }>('SELECT total_bytes FROM asset_usage WHERE id = 1')
      .toArray()
    if (usageRow.length > 0) {
      return usageRow[0].total_bytes
    }
    // Lazy one-time backfill for new or legacy DOs created before asset_usage existed.
    const sumRow = this.ctx.storage.sql
      .exec<{ total: number }>('SELECT COALESCE(SUM(size_bytes), 0) AS total FROM assets')
      .one()
    this.ctx.storage.sql.exec(
      'INSERT OR REPLACE INTO asset_usage (id, total_bytes) VALUES (1, ?)',
      sumRow.total,
    )
    return sumRow.total
  }

  private broadcastPoke(senderDeviceId: string, latestSeq: number): void {
    const sockets = this.ctx.getWebSockets()
    if (sockets.length === 0) return
    const message: PokeMessage = { type: 'poke', latestSeq }
    const payload = JSON.stringify(message)
    for (const ws of sockets) {
      let targetDeviceId: string | undefined
      try {
        const attachment = ws.deserializeAttachment() as WsAttachment | null
        targetDeviceId = attachment?.deviceId
      } catch {
        targetDeviceId = undefined
      }
      if (!targetDeviceId) {
        const tags = this.ctx.getTags(ws)
        targetDeviceId = tags[0]
      }
      if (targetDeviceId === senderDeviceId) continue
      try {
        ws.send(payload)
      } catch {
        try {
          ws.close(1011, 'poke send failed')
        } catch {
          // Socket already closed.
        }
      }
    }
  }

  async fetch(request: Request): Promise<Response> {
    if (request.headers.get('Upgrade')?.toLowerCase() !== 'websocket') {
      return new Response(JSON.stringify({ error: 'upgrade_required' }), {
        status: 426,
        headers: { 'Content-Type': 'application/json', Upgrade: 'websocket' },
      })
    }
    const deviceId = request.headers.get('X-Rem-Device-Id')?.trim() ?? ''
    if (deviceId.length === 0 || deviceId.length > MAX_DEVICE_ID_LENGTH) {
      return new Response(JSON.stringify({ error: 'invalid_device_id' }), {
        status: 400,
        headers: { 'Content-Type': 'application/json' },
      })
    }
    const userId = request.headers.get('X-Rem-User-Id')?.trim()
    if (userId) {
      this.ensureSchema()
      this.persistUserId(userId)
    }

    const pair = new WebSocketPair()
    const [client, server] = Object.values(pair)
    this.ctx.acceptWebSocket(server, [deviceId])
    server.serializeAttachment({ deviceId } satisfies WsAttachment)

    return new Response(null, {
      status: 101,
      webSocket: client,
    })
  }

  async webSocketMessage(_ws: WebSocket, _message: string | ArrayBuffer): Promise<void> {
    // Keepalive "ping" -> "pong" is handled via setWebSocketAutoResponse without waking the DO.
    // Ignore any other client messages safely.
  }

  async webSocketClose(
    ws: WebSocket,
    code: number,
    reason: string,
    _wasClean: boolean,
  ): Promise<void> {
    try {
      const safeCode = code === 1005 || code === 1006 || code === 1015 ? 1000 : code
      ws.close(safeCode, reason)
    } catch {
      // Socket already closed.
    }
  }

  async webSocketError(ws: WebSocket, _error: unknown): Promise<void> {
    try {
      ws.close(1011, 'websocket error')
    } catch {
      // Socket already closed.
    }
  }

  async getBootstrapInfo(): Promise<BootstrapInfo | null> {
    this.ensureSchema()
    const snapshotR2Key = this.getMeta('snapshot_r2_key')
    const snapshotSeq = this.getMetaInt('snapshot_seq', 0)
    if (!snapshotR2Key || snapshotSeq <= 0) {
      return null
    }
    return { snapshotSeq, snapshotR2Key }
  }

  async sync(req: SyncRequest, userId?: string): Promise<SyncRpcResult> {
    if (!req || typeof req !== 'object' || Array.isArray(req)) {
      return validationFailure('invalid sync request body')
    }
    if (
      typeof req.deviceId !== 'string' ||
      req.deviceId.trim().length === 0 ||
      req.deviceId.length > MAX_DEVICE_ID_LENGTH
    ) {
      return validationFailure('invalid deviceId')
    }
    if (!Number.isSafeInteger(req.sinceSeq) || req.sinceSeq < 0) {
      return validationFailure('invalid sinceSeq')
    }
    if (!Array.isArray(req.ops) || req.ops.length > MAX_PUSH_BATCH) {
      return validationFailure(`ops batch must contain at most ${MAX_PUSH_BATCH} entries`)
    }
    const rawLimit = req.limit ?? DEFAULT_SYNC_LIMIT
    if (!Number.isSafeInteger(rawLimit) || rawLimit < 1) {
      return validationFailure('invalid limit')
    }
    const limit = Math.min(rawLimit, MAX_SYNC_LIMIT)

    const preparedOps: Array<{ opId: string; payloadJson: string }> = []
    let totalBatchBytes = 0

    for (const op of req.ops) {
      if (!op || typeof op !== 'object' || Array.isArray(op)) {
        return validationFailure('invalid op entry')
      }
      if (typeof op.opId !== 'string' || op.opId.trim().length === 0 || op.opId.length > 128) {
        return validationFailure('invalid opId')
      }
      if (!op.payload || typeof op.payload !== 'object' || Array.isArray(op.payload)) {
        return validationFailure('invalid op payload')
      }
      if (
        typeof op.payload.type !== 'string' ||
        op.payload.type.trim().length === 0 ||
        op.payload.type.length > 64
      ) {
        return validationFailure('op payload must include valid string type')
      }
      if (typeof op.payload.hlc !== 'string' || !HLC_RE.test(op.payload.hlc)) {
        return validationFailure('op payload must include canonical hlc')
      }
      let payloadJson: string
      try {
        payloadJson = JSON.stringify(op.payload)
      } catch {
        return validationFailure('op payload is not serializable')
      }
      const opBytes = TEXT_ENCODER.encode(payloadJson).byteLength
      if (opBytes > MAX_OP_PAYLOAD_BYTES) {
        return validationFailure(`op payload exceeds maximum size of ${MAX_OP_PAYLOAD_BYTES} bytes`)
      }
      totalBatchBytes += opBytes
      if (totalBatchBytes > MAX_BATCH_PAYLOAD_BYTES) {
        return validationFailure(
          `ops batch exceeds maximum serialized size of ${MAX_BATCH_PAYLOAD_BYTES} bytes`,
        )
      }
      preparedOps.push({ opId: op.opId, payloadJson })
    }

    this.ensureSchema()
    const now = Date.now()
    const compactAfterOps = parsePositiveInt(this.env.COMPACT_AFTER_OPS, DEFAULT_COMPACT_AFTER_OPS)
    const txResult = this.ctx.storage.transactionSync((): {
      response: SyncResponse
      insertedCount: number
      shouldScheduleCompaction: boolean
    } => {
      this.persistUserId(userId)

      let latestSeq = this.readLatestSeq()
      const ackedOpIds: string[] = []
      let insertedCount = 0

      for (const op of preparedOps) {
        // Check existence before INSERT so retried opIds never burn AUTOINCREMENT seq values.
        const existing = this.ctx.storage.sql
          .exec<{ op_id: string }>('SELECT op_id FROM ops WHERE op_id = ?', op.opId)
          .toArray()
        if (existing.length === 0) {
          this.ctx.storage.sql.exec(
            'INSERT INTO ops(op_id, device_id, payload, created_at) VALUES (?, ?, ?, ?)',
            op.opId,
            req.deviceId,
            op.payloadJson,
            now,
          )
          const inserted = this.ctx.storage.sql
            .exec<{ seq: number }>('SELECT last_insert_rowid() AS seq')
            .one()
          if (inserted.seq > latestSeq) {
            latestSeq = inserted.seq
          }
          insertedCount += 1
        }
        ackedOpIds.push(op.opId)
      }

      if (insertedCount > 0) {
        this.setMeta('latest_seq', String(latestSeq))
      }

      const minRetainedSeq = this.getMetaInt('min_retained_seq', 0)
      const snapshotSeq = this.getMetaInt('snapshot_seq', 0)
      const snapshotR2Key = this.getMeta('snapshot_r2_key')

      let shouldScheduleCompaction = false
      if (
        insertedCount > 0 &&
        this.getMeta('user_id') !== null &&
        this.getMeta('compaction_scheduled') !== '1' &&
        latestSeq - snapshotSeq >= compactAfterOps
      ) {
        this.setMeta('compaction_scheduled', '1')
        shouldScheduleCompaction = true
      }

      if (snapshotR2Key && snapshotSeq > 0 && req.sinceSeq < minRetainedSeq) {
        return {
          insertedCount,
          shouldScheduleCompaction,
          response: {
            latestSeq,
            hasMore: false,
            ackedOpIds,
            ops: [],
            bootstrapRequired: true,
            snapshotSeq,
          },
        }
      }

      const rows = this.ctx.storage.sql
        .exec<{ seq: number; op_id: string; device_id: string; payload: string }>(
          'SELECT seq, op_id, device_id, payload FROM ops WHERE seq > ? AND device_id != ? ORDER BY seq ASC LIMIT ?',
          req.sinceSeq,
          req.deviceId,
          limit + 1,
        )
        .toArray()

      const hasMore = rows.length > limit
      const pageRows = hasMore ? rows.slice(0, limit) : rows
      const ops: RemoteSyncOp[] = pageRows.map((row) => ({
        seq: row.seq,
        opId: row.op_id,
        deviceId: row.device_id,
        payload: JSON.parse(row.payload) as Record<string, unknown>,
      }))

      return {
        insertedCount,
        shouldScheduleCompaction,
        response: {
          latestSeq,
          hasMore,
          ackedOpIds,
          ops,
        },
      }
    })

    const gen = this.generation
    if (txResult.insertedCount > 0) {
      this.broadcastPoke(req.deviceId, txResult.response.latestSeq)
    }

    if (txResult.shouldScheduleCompaction) {
      try {
        await this.ctx.storage.setAlarm(Date.now() + COMPACTION_ALARM_DELAY_MS)
      } catch (err) {
        if (this.generation === gen && this.schemaReady) {
          this.setMeta('compaction_scheduled', '0')
        }
        console.error('[sync-server] failed to schedule compaction alarm:', err)
      }
    }

    return { status: 'ok', response: txResult.response }
  }

  async alarm(): Promise<void> {
    await this.runCompaction(Date.now())
  }

  async runCompaction(nowMs = Date.now()): Promise<{
    compacted: boolean
    snapshotSeq: number
    prunedUpToSeq: number
    deletedAssets: string[]
  }> {
    this.ensureSchema()
    const gen = this.generation
    const isStale = () => this.generation !== gen || !this.schemaReady
    const abortedResult = {
      compacted: false,
      snapshotSeq: 0,
      prunedUpToSeq: 0,
      deletedAssets: [] as string[],
    }

    try {
      const userId = this.getMeta('user_id')
      const prevSnapshotSeq = this.getMetaInt('snapshot_seq', 0)
      const prevSnapshotKey = this.getMeta('snapshot_r2_key')
      const prevMinRetainedSeq = this.getMetaInt('min_retained_seq', 0)
      const targetSeq = this.readLatestSeq()

      if (!userId || targetSeq <= prevSnapshotSeq) {
        this.setMeta('compaction_scheduled', '0')
        return {
          compacted: false,
          snapshotSeq: prevSnapshotSeq,
          prunedUpToSeq: prevMinRetainedSeq,
          deletedAssets: [],
        }
      }

      let prevSnapshotJson: string | null = null
      let startSeq = prevSnapshotSeq

      if (prevSnapshotKey && prevSnapshotSeq > 0) {
        const prevObj = await this.env.ASSETS.get(prevSnapshotKey)
        if (isStale()) return abortedResult
        if (prevObj) {
          const prevBytes = new Uint8Array(await prevObj.arrayBuffer())
          if (isStale()) return abortedResult
          prevSnapshotJson = await gunzipBytesToString(prevBytes)
          if (isStale()) return abortedResult
        } else if (prevMinRetainedSeq === 0) {
          // Safe fallback: full op history is still in SQLite, so rebuild from scratch.
          prevSnapshotJson = null
          startSeq = 0
        } else {
          throw new Error(`missing base snapshot in R2: ${prevSnapshotKey}`)
        }
      }

      let snapshotJson: string
      let referencedAssets: Set<string>
      const reducer = createSnapshotReducer(prevSnapshotJson)
      try {
        let cursorSeq = startSeq
        while (cursorSeq < targetSeq) {
          const rows = this.ctx.storage.sql
            .exec<{ seq: number; payload: string }>(
              'SELECT seq, payload FROM ops WHERE seq > ? AND seq <= ? ORDER BY seq ASC LIMIT ?',
              cursorSeq,
              targetSeq,
              COMPACTION_CHUNK_SIZE,
            )
            .toArray()
          if (rows.length === 0) break
          const chunkJson =
            '[' + rows.map((row) => `{"seq":${row.seq},"payload":${row.payload}}`).join(',') + ']'
          reducer.push_ops_chunk_json(chunkJson)
          cursorSeq = rows[rows.length - 1].seq
        }
        snapshotJson = reducer.finish_json()
        const refs = JSON.parse(reducer.referenced_assets_json()) as string[]
        referencedAssets = new Set(refs)
      } finally {
        reducer.free()
      }

      const newSnapshotKey = snapshotR2Key(userId, targetSeq)
      const gzippedSnapshot = await gzipString(snapshotJson)
      if (isStale()) return abortedResult

      await this.env.ASSETS.put(newSnapshotKey, gzippedSnapshot, {
        httpMetadata: {
          contentType: 'application/json',
          contentEncoding: 'gzip',
        },
      })
      if (isStale()) {
        // Account deletion ran while the snapshot upload was in flight — best-effort clean up
        // so we never leave a zombie snapshot in R2.
        await this.env.ASSETS.delete(newSnapshotKey).catch(() => {})
        return abortedResult
      }

      const compactAfterOps = parsePositiveInt(
        this.env.COMPACT_AFTER_OPS,
        DEFAULT_COMPACT_AFTER_OPS,
      )
      const enableOpPruning = this.env.ENABLE_OP_PRUNING === 'true'
      const retainTailOps = parseNonNegativeInt(this.env.RETAIN_TAIL_OPS, DEFAULT_RETAIN_TAIL_OPS)
      const retainTailDays = parseNonNegativeInt(
        this.env.RETAIN_TAIL_DAYS,
        DEFAULT_RETAIN_TAIL_DAYS,
      )

      const metaUpdate = this.ctx.storage.transactionSync((): {
        prunedUpToSeq: number
        needsReschedule: boolean
      } => {
        const currentLatestSeq = Math.max(this.readLatestSeq(), targetSeq)
        this.setMeta('latest_seq', String(currentLatestSeq))
        this.setMeta('snapshot_seq', String(targetSeq))
        this.setMeta('snapshot_r2_key', newSnapshotKey)

        let nextMinRetainedSeq = this.getMetaInt('min_retained_seq', 0)

        if (enableOpPruning) {
          const maxPruneByCount = Math.min(targetSeq, currentLatestSeq - retainTailOps)
          let maxPruneByAge = targetSeq
          if (retainTailDays > 0) {
            const ageCutoffMs = nowMs - retainTailDays * MS_PER_DAY
            const firstRecent = this.ctx.storage.sql
              .exec<{ first_seq: number | null }>(
                'SELECT MIN(seq) AS first_seq FROM ops WHERE created_at >= ?',
                ageCutoffMs,
              )
              .one()
            if (firstRecent.first_seq !== null) {
              maxPruneByAge = firstRecent.first_seq - 1
            }
          }
          const pruneSeq = Math.min(maxPruneByCount, maxPruneByAge)
          if (pruneSeq > 0) {
            this.ctx.storage.sql.exec('DELETE FROM ops WHERE seq <= ?', pruneSeq)
            nextMinRetainedSeq = Math.max(nextMinRetainedSeq, pruneSeq)
            this.setMeta('min_retained_seq', String(nextMinRetainedSeq))
          }
        }

        const needsReschedule = currentLatestSeq - targetSeq >= compactAfterOps
        this.setMeta('compaction_scheduled', needsReschedule ? '1' : '0')
        return { prunedUpToSeq: nextMinRetainedSeq, needsReschedule }
      })

      if (metaUpdate.needsReschedule) {
        try {
          await this.ctx.storage.setAlarm(Date.now() + COMPACTION_ALARM_DELAY_MS)
        } catch (err) {
          if (!isStale()) {
            this.setMeta('compaction_scheduled', '0')
          }
          console.error('[sync-server] failed to reschedule compaction alarm:', err)
        }
        if (isStale()) return abortedResult
      }

      // Best-effort cleanup of superseded snapshot in R2.
      if (prevSnapshotKey && prevSnapshotKey !== newSnapshotKey) {
        try {
          await this.env.ASSETS.delete(prevSnapshotKey)
        } catch (err) {
          console.error('[sync-server] best-effort delete of old R2 snapshot failed:', err)
        }
        if (isStale()) return abortedResult
      }

      // Slice 5: Grace-period orphan asset GC.
      // Scan any uncompacted tail ops (seq > targetSeq) so assets referenced by recent ops are protected.
      let tailCursor = targetSeq
      while (true) {
        const tailRows = this.ctx.storage.sql
          .exec<{ seq: number; payload: string }>(
            'SELECT seq, payload FROM ops WHERE seq > ? ORDER BY seq ASC LIMIT ?',
            tailCursor,
            COMPACTION_CHUNK_SIZE,
          )
          .toArray()
        if (tailRows.length === 0) break
        for (const row of tailRows) {
          for (const hash of extractAssetRefsFromText(row.payload)) {
            referencedAssets.add(hash)
          }
        }
        tailCursor = tailRows[tailRows.length - 1].seq
      }

      const graceDays = parseNonNegativeInt(
        this.env.ASSET_GC_GRACE_DAYS,
        DEFAULT_ASSET_GC_GRACE_DAYS,
      )
      const gcCutoffMs = nowMs - graceDays * MS_PER_DAY
      const candidateAssets = this.ctx.storage.sql
        .exec<{ hash: string; size_bytes: number }>(
          'SELECT hash, size_bytes FROM assets WHERE uploaded_at < ?',
          gcCutoffMs,
        )
        .toArray()

      const orphanAssets = candidateAssets.filter((asset) => !referencedAssets.has(asset.hash))
      const deletedAssets: string[] = []

      if (orphanAssets.length > 0) {
        const R2_DELETE_BATCH = 1000
        for (let i = 0; i < orphanAssets.length; i += R2_DELETE_BATCH) {
          if (isStale()) return abortedResult
          const batch = orphanAssets.slice(i, i + R2_DELETE_BATCH)
          const r2Keys = batch.map((asset) => assetR2Key(userId, asset.hash))
          await this.env.ASSETS.delete(r2Keys)
          if (isStale()) return abortedResult

          this.ctx.storage.transactionSync(() => {
            let freedBytes = 0
            for (const asset of batch) {
              const existing = this.ctx.storage.sql
                .exec<{ size_bytes: number }>(
                  'SELECT size_bytes FROM assets WHERE hash = ?',
                  asset.hash,
                )
                .toArray()
              if (existing.length > 0) {
                this.ctx.storage.sql.exec('DELETE FROM assets WHERE hash = ?', asset.hash)
                freedBytes += existing[0].size_bytes
                deletedAssets.push(asset.hash)
              }
            }
            if (freedBytes > 0) {
              const currentTotal = this.readTotalAssetBytes()
              const nextTotal = Math.max(0, currentTotal - freedBytes)
              this.ctx.storage.sql.exec(
                'UPDATE asset_usage SET total_bytes = ? WHERE id = 1',
                nextTotal,
              )
            }
          })
        }
      }

      return {
        compacted: true,
        snapshotSeq: targetSeq,
        prunedUpToSeq: metaUpdate.prunedUpToSeq,
        deletedAssets,
      }
    } catch (err) {
      if (isStale()) return abortedResult
      this.setMeta('compaction_scheduled', '0')
      throw err
    }
  }

  async missingAssets(hashes: string[]): Promise<string[]> {
    this.ensureSchema()
    const missing: string[] = []
    const seen = new Set<string>()
    for (const hash of hashes) {
      if (seen.has(hash)) continue
      seen.add(hash)
      const row = this.ctx.storage.sql
        .exec<{ hash: string }>('SELECT hash FROM assets WHERE hash = ?', hash)
        .toArray()
      if (row.length === 0) {
        missing.push(hash)
      }
    }
    return missing
  }

  async checkAssetQuota(
    hash: string,
    sizeBytes: number,
    maxQuotaBytes: number,
  ): Promise<QuotaCheckResult> {
    if (
      !Number.isSafeInteger(sizeBytes) ||
      sizeBytes < 0 ||
      !Number.isSafeInteger(maxQuotaBytes) ||
      maxQuotaBytes < 0
    ) {
      throw new Error('invalid asset quota parameters')
    }
    this.ensureSchema()
    return this.ctx.storage.transactionSync(() => {
      const existing = this.ctx.storage.sql
        .exec<{ hash: string }>('SELECT hash FROM assets WHERE hash = ?', hash)
        .toArray()
      if (existing.length > 0) {
        return { status: 'already_present' }
      }
      const totalBytes = this.readTotalAssetBytes()
      if (sizeBytes > maxQuotaBytes - totalBytes) {
        return { status: 'quota_exceeded' }
      }
      return { status: 'allowed' }
    })
  }

  async recordAsset(
    hash: string,
    sizeBytes: number,
    mime: string,
    maxQuotaBytes: number,
  ): Promise<QuotaCheckResult> {
    if (
      !Number.isSafeInteger(sizeBytes) ||
      sizeBytes < 0 ||
      !Number.isSafeInteger(maxQuotaBytes) ||
      maxQuotaBytes < 0
    ) {
      throw new Error('invalid asset quota parameters')
    }
    this.ensureSchema()
    const now = Date.now()
    return this.ctx.storage.transactionSync(() => {
      const existing = this.ctx.storage.sql
        .exec<{ hash: string }>('SELECT hash FROM assets WHERE hash = ?', hash)
        .toArray()
      if (existing.length > 0) {
        return { status: 'already_present' }
      }
      const totalBytes = this.readTotalAssetBytes()
      if (sizeBytes > maxQuotaBytes - totalBytes) {
        return { status: 'quota_exceeded' }
      }
      this.ctx.storage.sql.exec(
        'INSERT INTO assets(hash, size_bytes, mime, uploaded_at) VALUES (?, ?, ?, ?)',
        hash,
        sizeBytes,
        mime,
        now,
      )
      this.ctx.storage.sql.exec(
        'UPDATE asset_usage SET total_bytes = total_bytes + ? WHERE id = 1',
        sizeBytes,
      )
      return { status: 'allowed' }
    })
  }

  async deleteAllData(): Promise<void> {
    await this.ctx.blockConcurrencyWhile(async () => {
      this.generation += 1
      for (const ws of this.ctx.getWebSockets()) {
        try {
          ws.close(WS_CLOSE_ACCOUNT_DELETED, 'account_deleted')
        } catch {
          // Socket already closed.
        }
      }
      try {
        await this.ctx.storage.deleteAlarm()
        await this.ctx.storage.deleteAll()
      } finally {
        this.schemaReady = false
        this.cachedUserId = null
      }
    })
  }
}
