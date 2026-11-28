import { DurableObject } from 'cloudflare:workers'
import type { Env, QuotaCheckResult, RemoteSyncOp, SyncRequest, SyncRpcResult } from './types'

const MAX_SYNC_LIMIT = 500
const DEFAULT_SYNC_LIMIT = 500
const MAX_PUSH_BATCH = 500
const MAX_OP_PAYLOAD_BYTES = 256 * 1024
const MAX_BATCH_PAYLOAD_BYTES = 2 * 1024 * 1024
const HLC_RE = /^\d{13}-\d{4}-[a-z0-9]{8}$/
const TEXT_ENCODER = new TextEncoder()

function validationFailure(error: string): SyncRpcResult {
  return { status: 'invalid', error }
}

export class UserSyncDO extends DurableObject<Env> {
  private schemaReady = false

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env)
    this.ensureSchema()
  }

  private ensureSchema(): void {
    if (this.schemaReady) return
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
    `)
    this.schemaReady = true
  }

  async sync(req: SyncRequest): Promise<SyncRpcResult> {
    this.ensureSchema()
    if (!req || typeof req !== 'object' || Array.isArray(req)) {
      return validationFailure('invalid sync request body')
    }
    if (typeof req.deviceId !== 'string' || req.deviceId.trim().length === 0 || req.deviceId.length > 64) {
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

    const now = Date.now()
    const response = this.ctx.storage.transactionSync(() => {
      const ackedOpIds: string[] = []
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
        }
        ackedOpIds.push(op.opId)
      }

      const rows = this.ctx.storage.sql
        .exec<{ seq: number; op_id: string; device_id: string; payload: string }>(
          'SELECT seq, op_id, device_id, payload FROM ops WHERE seq > ? ORDER BY seq ASC LIMIT ?',
          req.sinceSeq,
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

      const latestRow = this.ctx.storage.sql
        .exec<{ latest_seq: number }>('SELECT COALESCE(MAX(seq), 0) AS latest_seq FROM ops')
        .one()

      return {
        latestSeq: latestRow.latest_seq,
        hasMore,
        ackedOpIds,
        ops,
      }
    })
    return { status: 'ok', response }
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
    this.ensureSchema()
    return this.ctx.storage.transactionSync(() => {
      const existing = this.ctx.storage.sql
        .exec<{ hash: string }>('SELECT hash FROM assets WHERE hash = ?', hash)
        .toArray()
      if (existing.length > 0) {
        return { status: 'already_present' }
      }
      const totalRow = this.ctx.storage.sql
        .exec<{ total: number }>('SELECT COALESCE(SUM(size_bytes), 0) AS total FROM assets')
        .one()
      if (totalRow.total + sizeBytes > maxQuotaBytes) {
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
    this.ensureSchema()
    const now = Date.now()
    return this.ctx.storage.transactionSync(() => {
      const existing = this.ctx.storage.sql
        .exec<{ hash: string }>('SELECT hash FROM assets WHERE hash = ?', hash)
        .toArray()
      if (existing.length > 0) {
        return { status: 'already_present' }
      }
      const totalRow = this.ctx.storage.sql
        .exec<{ total: number }>('SELECT COALESCE(SUM(size_bytes), 0) AS total FROM assets')
        .one()
      if (totalRow.total + sizeBytes > maxQuotaBytes) {
        return { status: 'quota_exceeded' }
      }
      this.ctx.storage.sql.exec(
        'INSERT OR IGNORE INTO assets(hash, size_bytes, mime, uploaded_at) VALUES (?, ?, ?, ?)',
        hash,
        sizeBytes,
        mime,
        now,
      )
      return { status: 'allowed' }
    })
  }

  async deleteAllData(): Promise<void> {
    await this.ctx.storage.deleteAll()
    this.schemaReady = false
  }
}
