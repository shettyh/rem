import { DurableObject } from 'cloudflare:workers'
import type { Env, QuotaCheckResult, RemoteSyncOp, SyncRequest, SyncResponse } from './types'

const MAX_SYNC_LIMIT = 500
const DEFAULT_SYNC_LIMIT = 500
const MAX_PUSH_BATCH = 500

export class UserSyncDO extends DurableObject<Env> {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env)
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
  }

  async sync(req: SyncRequest): Promise<SyncResponse> {
    if (typeof req.deviceId !== 'string' || req.deviceId.trim().length === 0 || req.deviceId.length > 64) {
      throw new Error('invalid deviceId')
    }
    if (!Number.isInteger(req.sinceSeq) || req.sinceSeq < 0) {
      throw new Error('invalid sinceSeq')
    }
    if (!Array.isArray(req.ops) || req.ops.length > MAX_PUSH_BATCH) {
      throw new Error(`ops batch must contain at most ${MAX_PUSH_BATCH} entries`)
    }
    const rawLimit = req.limit ?? DEFAULT_SYNC_LIMIT
    if (!Number.isInteger(rawLimit) || rawLimit < 1) {
      throw new Error('invalid limit')
    }
    const limit = Math.min(rawLimit, MAX_SYNC_LIMIT)

    for (const op of req.ops) {
      if (typeof op.opId !== 'string' || op.opId.trim().length === 0 || op.opId.length > 128) {
        throw new Error('invalid opId')
      }
      if (!op.payload || typeof op.payload !== 'object' || Array.isArray(op.payload)) {
        throw new Error('invalid op payload')
      }
      if (typeof op.payload.type !== 'string' || typeof op.payload.hlc !== 'string') {
        throw new Error('op payload must include string type and hlc')
      }
    }

    const now = Date.now()
    return this.ctx.storage.transactionSync(() => {
      const ackedOpIds: string[] = []
      for (const op of req.ops) {
        // Check existence before INSERT so retried opIds never burn AUTOINCREMENT seq values.
        const existing = this.ctx.storage.sql
          .exec<{ op_id: string }>('SELECT op_id FROM ops WHERE op_id = ?', op.opId)
          .toArray()
        if (existing.length === 0) {
          this.ctx.storage.sql.exec(
            'INSERT INTO ops(op_id, device_id, payload, created_at) VALUES (?, ?, ?, ?)',
            op.opId,
            req.deviceId,
            JSON.stringify(op.payload),
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
  }

  async missingAssets(hashes: string[]): Promise<string[]> {
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
  }
}
