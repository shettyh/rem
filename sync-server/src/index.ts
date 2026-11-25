import { handleAppleWebhook, handleVerifyAppleTransaction } from './appleBilling'
import { authenticateRequest, sha256HexBytes } from './auth'
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
import type { Env, MissingAssetsRequest, SyncRequest } from './types'
import { UserSyncDO } from './userSyncDo'

export { UserSyncDO }

const SHA256_HEX_RE = /^[0-9a-f]{64}$/
const DEFAULT_MAX_ASSET_BYTES = 10 * 1024 * 1024
const DEFAULT_MAX_USER_QUOTA_BYTES = 500 * 1024 * 1024
const MAX_MISSING_HASHES = 500

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
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

    if (url.pathname === '/v1/account') {
      if (request.method === 'GET') {
        const info = await getAccountInfo(env, auth.userId, auth.authMethod)
        return jsonResponse(info)
      }
      if (request.method === 'DELETE') {
        await deleteAccountData(env, auth.userId)
        return jsonResponse({ deleted: true })
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

    const stub = env.USER_SYNC.get(env.USER_SYNC.idFromName(auth.userId))

    if (request.method === 'POST' && url.pathname === '/v1/sync') {
      let body: SyncRequest
      try {
        body = (await request.json()) as SyncRequest
      } catch {
        return jsonResponse({ error: 'invalid_json' }, 400)
      }
      try {
        const result = await stub.sync(body)
        return jsonResponse(result)
      } catch (err) {
        const message = err instanceof Error ? err.message : 'invalid sync request'
        return jsonResponse({ error: 'invalid_request', message }, 400)
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
      const missing = await stub.missingAssets(body.hashes)
      return jsonResponse({ missing })
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

        const precheck = await stub.checkAssetQuota(hash, bytes.byteLength, maxUserQuotaBytes)
        if (precheck.status === 'quota_exceeded') {
          return jsonResponse({ error: 'quota_exceeded' }, 413)
        }
        if (precheck.status === 'already_present') {
          return jsonResponse({ hash, sizeBytes: bytes.byteLength, mime })
        }

        await env.ASSETS.put(assetR2Key(auth.userId, hash), bytes, {
          httpMetadata: { contentType: mime },
        })
        const recorded = await stub.recordAsset(hash, bytes.byteLength, mime, maxUserQuotaBytes)
        if (recorded.status === 'quota_exceeded') {
          await env.ASSETS.delete(assetR2Key(auth.userId, hash))
          return jsonResponse({ error: 'quota_exceeded' }, 413)
        }

        return jsonResponse({ hash, sizeBytes: bytes.byteLength, mime })
      }

      if (request.method === 'GET') {
        const object = await env.ASSETS.get(assetR2Key(auth.userId, hash))
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
