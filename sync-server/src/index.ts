import { handleAppleWebhook, handleVerifyAppleTransaction } from './appleBilling'
import { authenticateRequest, sha256HexBytes, verifyJwtAccountExists } from './auth'
import { renderCloudPage } from './cloudPage'
import { checkSyncEntitlement, getAccountInfo, handleStartTrial } from './entitlements'
import { renderPrivacyPage, renderSupportPage, renderTermsPage } from './legalPages'
import { cleanupExpiredRecords } from './maintenance'
import {
  deleteAccountData,
  getAuthProviders,
  handleAppleNativeAuth,
  handleLogout,
  handleOAuthCallback,
  handleOAuthStart,
  handleTokenExchange,
  handleTokenRefresh,
} from './oauth'
import {
  handleCreatePolarCheckout,
  handleCreatePolarPortal,
  handlePolarWebhook,
} from './polar'
import type { Env, MissingAssetsRequest, SyncRequest, SyncRpcResult } from './types'
import { UserSyncDO } from './userSyncDo'

export { UserSyncDO }

const SHA256_HEX_RE = /^[0-9a-f]{64}$/
const DEFAULT_MAX_ASSET_BYTES = 10 * 1024 * 1024
const DEFAULT_MAX_USER_QUOTA_BYTES = 500 * 1024 * 1024
const MAX_MISSING_HASHES = 500
const MAX_SYNC_WIRE_BYTES = 4 * 1024 * 1024
const MAX_SYNC_DECOMPRESSED_BYTES = 8 * 1024 * 1024
const MAX_DEVICE_ID_LENGTH = 64
const TEXT_DECODER = new TextDecoder()

type ReadBodyResult =
  | { status: 'ok'; text: string }
  | { status: 'too_large' }
  | { status: 'invalid_gzip' }
  | { status: 'unsupported_encoding' }

function concatChunks(chunks: Uint8Array[], totalBytes: number): Uint8Array {
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

async function readStreamWithCap(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  maxBytes: number,
  initialChunks: Uint8Array[] = [],
  initialBytes = 0,
): Promise<{ status: 'ok'; bytes: Uint8Array } | { status: 'too_large' } | { status: 'error' }> {
  const chunks = [...initialChunks]
  let totalBytes = initialBytes
  try {
    while (true) {
      const readRes = await reader.read().then(
        (step) => ({ ok: true as const, step }),
        () => ({ ok: false as const }),
      )
      if (!readRes.ok) {
        return { status: 'error' }
      }
      const { value, done } = readRes.step
      if (done) break
      if (!value || value.byteLength === 0) continue
      totalBytes += value.byteLength
      if (totalBytes > maxBytes) {
        await reader.cancel().catch(() => {})
        return { status: 'too_large' }
      }
      chunks.push(value)
    }
    return { status: 'ok', bytes: concatChunks(chunks, totalBytes) }
  } catch {
    return { status: 'error' }
  } finally {
    try {
      reader.releaseLock()
    } catch {
      // ignore
    }
    reader.closed.catch(() => {})
  }
}

async function readSyncRequestText(request: Request): Promise<ReadBodyResult> {
  const rawEncoding = request.headers.get('Content-Encoding')
  const encodings = rawEncoding
    ? rawEncoding
        .split(',')
        .map((part) => part.trim().toLowerCase())
        .filter((part) => part.length > 0 && part !== 'identity')
    : []

  const contentLengthHeader = request.headers.get('Content-Length')
  const declaredLen = contentLengthHeader ? Number.parseInt(contentLengthHeader, 10) : Number.NaN

  if (encodings.length === 0) {
    if (Number.isFinite(declaredLen) && declaredLen > MAX_SYNC_WIRE_BYTES) {
      return { status: 'too_large' }
    }
    if (!request.body) {
      return { status: 'ok', text: '' }
    }
    const read = await readStreamWithCap(request.body.getReader(), MAX_SYNC_WIRE_BYTES)
    if (read.status === 'too_large') return read
    if (read.status === 'error') return { status: 'ok', text: '' }
    return { status: 'ok', text: TEXT_DECODER.decode(read.bytes) }
  }

  if (encodings.length !== 1 || (encodings[0] !== 'gzip' && encodings[0] !== 'x-gzip')) {
    return { status: 'unsupported_encoding' }
  }

  if (Number.isFinite(declaredLen) && declaredLen > MAX_SYNC_DECOMPRESSED_BYTES) {
    return { status: 'too_large' }
  }
  if (!request.body) {
    return { status: 'ok', text: '' }
  }

  // Peek the first 2 bytes to detect whether the body is raw gzip (magic 0x1f 0x8b)
  // or was already auto-decompressed by the runtime/proxy.
  const bodyReader = request.body.getReader()
  const prefixChunks: Uint8Array[] = []
  let prefixBytes = 0
  let reachedEof = false

  while (prefixBytes < 2) {
    const { value, done } = await bodyReader.read()
    if (done) {
      reachedEof = true
      break
    }
    if (!value || value.byteLength === 0) continue
    prefixBytes += value.byteLength
    if (prefixBytes > MAX_SYNC_DECOMPRESSED_BYTES) {
      await bodyReader.cancel().catch(() => {})
      bodyReader.releaseLock()
      return { status: 'too_large' }
    }
    prefixChunks.push(value)
  }

  const prefix = concatChunks(prefixChunks, prefixBytes)
  const isGzipMagic = prefix.byteLength >= 2 && prefix[0] === 0x1f && prefix[1] === 0x8b

  if (!isGzipMagic) {
    if (reachedEof) {
      bodyReader.releaseLock()
      return { status: 'ok', text: TEXT_DECODER.decode(prefix) }
    }
    const read = await readStreamWithCap(bodyReader, MAX_SYNC_DECOMPRESSED_BYTES, [prefix], prefix.byteLength)
    if (read.status === 'too_large') return read
    if (read.status === 'error') return { status: 'ok', text: '' }
    return { status: 'ok', text: TEXT_DECODER.decode(read.bytes) }
  }

  if (
    (Number.isFinite(declaredLen) && declaredLen > MAX_SYNC_WIRE_BYTES) ||
    prefix.byteLength > MAX_SYNC_WIRE_BYTES
  ) {
    await bodyReader.cancel().catch(() => {})
    bodyReader.releaseLock()
    return { status: 'too_large' }
  }

  let compressedBytes: Uint8Array
  if (reachedEof) {
    bodyReader.releaseLock()
    compressedBytes = prefix
  } else {
    const compressedRead = await readStreamWithCap(
      bodyReader,
      MAX_SYNC_WIRE_BYTES,
      [prefix],
      prefix.byteLength,
    )
    if (compressedRead.status === 'too_large') {
      return { status: 'too_large' }
    }
    if (compressedRead.status === 'error') {
      return { status: 'invalid_gzip' }
    }
    compressedBytes = compressedRead.bytes
  }

  // RFC 1952: minimal valid gzip member is 10-byte header + 8-byte trailer (18 bytes),
  // with compression method byte (index 2) === 8 (deflate) and reserved FLG bits 5..7 === 0.
  if (
    compressedBytes.byteLength < 18 ||
    compressedBytes[2] !== 0x08 ||
    (compressedBytes[3] & 0xe0) !== 0
  ) {
    return { status: 'invalid_gzip' }
  }

  const ds = new DecompressionStream('gzip')
  const writer = ds.writable.getWriter()
  writer.closed.catch(() => {})
  writer.ready.catch(() => {})
  const outReader = ds.readable.getReader()
  outReader.closed.catch(() => {})

  let stopWriting = false
  const writePromise = (async (): Promise<unknown> => {
    try {
      await writer.write(compressedBytes)
      if (!stopWriting) {
        await writer.close()
      }
      return null
    } catch (err) {
      return err
    } finally {
      try {
        writer.releaseLock()
      } catch {
        // ignore
      }
      writer.closed.catch(() => {})
    }
  })()

  const decompressed = await readStreamWithCap(outReader, MAX_SYNC_DECOMPRESSED_BYTES)
  if (decompressed.status === 'too_large') {
    stopWriting = true
    await writer.abort().catch(() => {})
    await writePromise
    return { status: 'too_large' }
  }

  const writeErr = await writePromise
  if (decompressed.status === 'error' || writeErr !== null) {
    return { status: 'invalid_gzip' }
  }

  return { status: 'ok', text: TEXT_DECODER.decode(decompressed.bytes) }
}

function jsonResponse(
  body: unknown,
  status = 200,
  extraHeaders?: Record<string, string>,
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json', ...extraHeaders },
  })
}

function parsePositiveInt(raw: string | undefined, fallback: number): number {
  if (!raw) return fallback
  const parsed = Number.parseInt(raw, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function assetR2Key(userId: string, hash: string): string {
  return `users/${userId}/assets/${hash}`
}

function isDurableObjectQuotaError(err: unknown): boolean {
  if (typeof err === 'object' && err !== null && (err as { overloaded?: unknown }).overloaded === true) {
    return true
  }
  const message = err instanceof Error ? err.message : String(err ?? '')
  return /exceeded allowed rows|free tier|daily free limit|storage (limit|quota) exceeded|SQLITE_FULL|overloaded/i.test(
    message,
  )
}

function storageErrorResponse(context: string, err: unknown): Response {
  console.error(`[sync-server] ${context} failed:`, err)
  if (isDurableObjectQuotaError(err)) {
    return jsonResponse(
      {
        error: 'storage_limit_exceeded',
        message: 'sync storage temporarily unavailable (server storage limit exceeded)',
      },
      503,
      { 'Cache-Control': 'no-store', 'Retry-After': '60' },
    )
  }
  return jsonResponse({ error: 'internal_error', message: 'sync storage error' }, 500)
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url)

    if (request.method === 'GET' && url.pathname === '/health') {
      return jsonResponse({ ok: true })
    }

    if (
      request.method === 'GET' &&
      (url.pathname === '/premium' ||
        url.pathname === '/premium/' ||
        url.pathname === '/cloud' ||
        url.pathname === '/cloud/')
    ) {
      return renderCloudPage(env)
    }

    if (request.method === 'GET' && (url.pathname === '/privacy' || url.pathname === '/privacy/')) {
      return renderPrivacyPage()
    }

    if (request.method === 'GET' && (url.pathname === '/terms' || url.pathname === '/terms/')) {
      return renderTermsPage()
    }

    if (request.method === 'GET' && (url.pathname === '/support' || url.pathname === '/support/')) {
      return renderSupportPage()
    }

    // Public auth routes
    if (request.method === 'GET' && url.pathname === '/v1/auth/providers') {
      return jsonResponse(getAuthProviders(env))
    }

    const oauthStartMatch = /^\/v1\/auth\/([a-z]+)\/start$/.exec(url.pathname)
    if (oauthStartMatch && request.method === 'GET') {
      return handleOAuthStart(request, env, oauthStartMatch[1])
    }

    const oauthCallbackMatch = /^\/v1\/auth\/([a-z]+)\/callback$/.exec(url.pathname)
    if (oauthCallbackMatch && (request.method === 'GET' || request.method === 'POST')) {
      return handleOAuthCallback(request, env, oauthCallbackMatch[1])
    }

    if (request.method === 'POST' && url.pathname === '/v1/auth/exchange') {
      return handleTokenExchange(request, env)
    }

    if (request.method === 'POST' && url.pathname === '/v1/auth/apple/native') {
      return handleAppleNativeAuth(request, env)
    }

    if (request.method === 'POST' && url.pathname === '/v1/auth/refresh') {
      return handleTokenRefresh(request, env)
    }

    if (request.method === 'POST' && url.pathname === '/v1/auth/logout') {
      return handleLogout(request, env)
    }

    if (request.method === 'POST' && url.pathname === '/v1/webhooks/polar') {
      return handlePolarWebhook(request, env)
    }

    if (request.method === 'POST' && url.pathname === '/v1/webhooks/apple') {
      return handleAppleWebhook(request, env)
    }

    if (request.method === 'POST' && url.pathname === '/v1/waitlist') {
      return jsonResponse(
        {
          error: 'waitlist_retired',
          message:
            'The early-access waitlist has been retired. Start a 30-day free trial at /premium.',
          premiumUrl: `${url.origin}/premium`,
        },
        410,
      )
    }

    // Authenticated routes
    const auth = await authenticateRequest(request, env)
    if (!auth) {
      return jsonResponse({ error: 'unauthorized' }, 401)
    }

    if (auth.authMethod === 'jwt') {
      const exists = await verifyJwtAccountExists(env, auth.userId)
      if (!exists) {
        return jsonResponse({ error: 'account_not_found' }, 401)
      }
    }

    if (url.pathname === '/v1/account') {
      if (request.method === 'GET') {
        const info = await getAccountInfo(env, auth.userId, auth.authMethod)
        return jsonResponse(info)
      }
      if (request.method === 'DELETE') {
        try {
          await deleteAccountData(env, auth.userId)
          return jsonResponse({ deleted: true })
        } catch (err) {
          return storageErrorResponse('DELETE /v1/account', err)
        }
      }
    }

    if (request.method === 'POST' && url.pathname === '/v1/trial/start') {
      return handleStartTrial(env, auth.userId, auth.authMethod)
    }

    if (request.method === 'POST' && url.pathname === '/v1/billing/polar/checkout') {
      return handleCreatePolarCheckout(request, env, auth.userId, auth.authMethod)
    }

    if (request.method === 'POST' && url.pathname === '/v1/billing/polar/portal') {
      return handleCreatePolarPortal(env, auth.userId, auth.authMethod)
    }

    if (request.method === 'POST' && url.pathname === '/v1/billing/apple/verify') {
      return handleVerifyAppleTransaction(request, env, auth.userId, auth.authMethod)
    }

    const entitlement = await checkSyncEntitlement(env, auth.userId, auth.authMethod)
    if (!entitlement.syncEnabled) {
      return jsonResponse(
        {
          error: 'subscription_required',
          state: entitlement.entitlement.state,
          message: 'Cloud sync requires an active rem Premium plan or free trial.',
          upgradeUrl: entitlement.upgradeUrl,
        },
        402,
      )
    }

    const getUserSyncStub = () => env.USER_SYNC.get(env.USER_SYNC.idFromName(auth.userId))

    if (request.method === 'GET' && url.pathname === '/v1/sync/ws') {
      if (request.headers.get('Upgrade')?.toLowerCase() !== 'websocket') {
        return jsonResponse(
          { error: 'upgrade_required', message: 'WebSocket upgrade required' },
          426,
          { Upgrade: 'websocket' },
        )
      }
      const deviceId = request.headers.get('X-Rem-Device-Id')?.trim() ?? ''
      if (deviceId.length === 0 || deviceId.length > MAX_DEVICE_ID_LENGTH) {
        return jsonResponse(
          { error: 'invalid_device_id', message: 'missing or invalid X-Rem-Device-Id header' },
          400,
        )
      }
      try {
        const wsHeaders = new Headers(request.headers)
        wsHeaders.set('X-Rem-Device-Id', deviceId)
        wsHeaders.set('X-Rem-User-Id', auth.userId)
        const wsRequest = new Request('https://user-sync.internal/ws', {
          method: 'GET',
          headers: wsHeaders,
        })
        return await getUserSyncStub().fetch(wsRequest)
      } catch (err) {
        return storageErrorResponse('GET /v1/sync/ws', err)
      }
    }

    if (request.method === 'GET' && url.pathname === '/v1/bootstrap') {
      let bootstrap
      try {
        bootstrap = await getUserSyncStub().getBootstrapInfo()
      } catch (err) {
        return storageErrorResponse('GET /v1/bootstrap', err)
      }
      if (!bootstrap) {
        return jsonResponse({ error: 'no_snapshot' }, 404)
      }
      let object
      try {
        object = await env.ASSETS.get(bootstrap.snapshotR2Key)
      } catch (err) {
        return storageErrorResponse('GET /v1/bootstrap R2 get', err)
      }
      if (!object) {
        return jsonResponse({ error: 'no_snapshot' }, 404)
      }
      const headers = new Headers()
      headers.set('Content-Type', 'application/json')
      headers.set('Content-Encoding', 'gzip')
      headers.set('X-Rem-Snapshot-Seq', String(bootstrap.snapshotSeq))
      headers.set('Cache-Control', 'private, no-cache')
      if (object.httpEtag) {
        headers.set('ETag', object.httpEtag)
      }
      return new Response(object.body, {
        status: 200,
        headers,
        encodeBody: 'manual',
      } as ResponseInit)
    }

    if (request.method === 'POST' && url.pathname === '/v1/sync') {
      const readResult = await readSyncRequestText(request)
      if (readResult.status === 'unsupported_encoding') {
        return jsonResponse(
          { error: 'unsupported_content_encoding', message: 'only gzip Content-Encoding is supported' },
          415,
        )
      }
      if (readResult.status === 'too_large') {
        return jsonResponse({ error: 'payload_too_large', message: 'sync request body too large' }, 413)
      }
      if (readResult.status === 'invalid_gzip') {
        return jsonResponse(
          { error: 'invalid_gzip', message: 'failed to decompress gzip sync request body' },
          400,
        )
      }
      let body: SyncRequest
      try {
        body = JSON.parse(readResult.text) as SyncRequest
      } catch {
        return jsonResponse({ error: 'invalid_json' }, 400)
      }
      try {
        const result = (await getUserSyncStub().sync(body, auth.userId)) as SyncRpcResult
        if (result.status === 'invalid') {
          return jsonResponse({ error: 'invalid_request', message: result.error }, 400)
        }
        return jsonResponse(result.response)
      } catch (err) {
        return storageErrorResponse('POST /v1/sync', err)
      }
    }

    if (request.method === 'POST' && url.pathname === '/v1/assets/missing') {
      let body: MissingAssetsRequest
      try {
        body = (await request.json()) as MissingAssetsRequest
      } catch {
        return jsonResponse({ error: 'invalid_json' }, 400)
      }
      if (
        !body ||
        !Array.isArray(body.hashes) ||
        body.hashes.length > MAX_MISSING_HASHES ||
        !body.hashes.every((h) => typeof h === 'string' && SHA256_HEX_RE.test(h))
      ) {
        return jsonResponse({ error: 'invalid_hashes' }, 400)
      }
      try {
        const missing = await getUserSyncStub().missingAssets(body.hashes)
        return jsonResponse({ missing })
      } catch (err) {
        return storageErrorResponse('POST /v1/assets/missing', err)
      }
    }

    const assetMatch = /^\/v1\/assets\/([0-9a-f]{64})$/.exec(url.pathname)
    if (assetMatch) {
      const hash = assetMatch[1]

      if (request.method === 'PUT') {
        const maxAssetBytes = parsePositiveInt(env.MAX_ASSET_BYTES, DEFAULT_MAX_ASSET_BYTES)
        const maxUserQuotaBytes = parsePositiveInt(
          env.MAX_USER_QUOTA_BYTES,
          DEFAULT_MAX_USER_QUOTA_BYTES,
        )

        const contentLengthHeader = request.headers.get('Content-Length')
        if (contentLengthHeader) {
          const declaredLen = Number.parseInt(contentLengthHeader, 10)
          if (Number.isFinite(declaredLen) && declaredLen > maxAssetBytes) {
            return jsonResponse({ error: 'asset_too_large' }, 413)
          }
        }

        const bytes = await request.arrayBuffer()
        if (bytes.byteLength === 0) {
          return jsonResponse({ error: 'empty_asset' }, 400)
        }
        if (bytes.byteLength > maxAssetBytes) {
          return jsonResponse({ error: 'asset_too_large' }, 413)
        }

        const computed = await sha256HexBytes(bytes)
        if (computed !== hash) {
          return jsonResponse({ error: 'hash_mismatch' }, 400)
        }

        const mime = (request.headers.get('Content-Type') ?? 'application/octet-stream')
          .split(';')[0]
          .trim() || 'application/octet-stream'

        let stub
        let precheck
        try {
          stub = getUserSyncStub()
          precheck = await stub.checkAssetQuota(hash, bytes.byteLength, maxUserQuotaBytes)
        } catch (err) {
          return storageErrorResponse('PUT /v1/assets checkAssetQuota', err)
        }
        if (precheck.status === 'quota_exceeded') {
          return jsonResponse({ error: 'quota_exceeded' }, 413)
        }
        if (precheck.status === 'already_present') {
          return jsonResponse({ hash, sizeBytes: bytes.byteLength, mime })
        }

        const r2Key = assetR2Key(auth.userId, hash)
        try {
          await env.ASSETS.put(r2Key, bytes, {
            httpMetadata: { contentType: mime },
          })
        } catch (err) {
          return storageErrorResponse('PUT /v1/assets R2 put', err)
        }

        let recorded
        try {
          recorded = await stub.recordAsset(hash, bytes.byteLength, mime, maxUserQuotaBytes)
        } catch (err) {
          // Do not blindly delete the R2 object on an unknown exception: a concurrent upload
          // may have already recorded this content-addressed hash in UserSyncDO.
          return storageErrorResponse('PUT /v1/assets recordAsset', err)
        }
        if (recorded.status === 'quota_exceeded') {
          try {
            await env.ASSETS.delete(r2Key)
          } catch (deleteErr) {
            console.error('[sync-server] PUT /v1/assets quota rollback R2 delete failed:', deleteErr)
          }
          return jsonResponse({ error: 'quota_exceeded' }, 413)
        }

        return jsonResponse({ hash, sizeBytes: bytes.byteLength, mime })
      }

      if (request.method === 'GET') {
        let object
        try {
          object = await env.ASSETS.get(assetR2Key(auth.userId, hash))
        } catch (err) {
          return storageErrorResponse('GET /v1/assets R2 get', err)
        }
        if (!object) {
          return jsonResponse({ error: 'not_found' }, 404)
        }
        const headers = new Headers()
        headers.set('Content-Type', object.httpMetadata?.contentType ?? 'application/octet-stream')
        headers.set('Cache-Control', 'private, max-age=31536000, immutable')
        headers.set('Content-Length', String(object.size))
        if (object.httpEtag) {
          headers.set('ETag', object.httpEtag)
        }
        return new Response(object.body, { status: 200, headers })
      }
    }

    return jsonResponse({ error: 'not_found' }, 404)
  },

  async scheduled(
    _controller: ScheduledController,
    env: Env,
    _ctx: ExecutionContext,
  ): Promise<void> {
    await cleanupExpiredRecords(env)
  },
} satisfies ExportedHandler<Env>
