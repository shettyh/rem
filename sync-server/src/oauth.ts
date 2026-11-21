import {
  decodeBase64Url,
  decodeBase64UrlToString,
  encodeBase64Url,
  pkceS256Challenge,
  randomToken,
  sha256HexString,
  signHs256Jwt,
  timingSafeStringEqual,
  verifyHs256Jwt,
} from './auth'
import type {
  AccountInfo,
  AuthProvidersResponse,
  AuthTokenResponse,
  Env,
  LogoutRequest,
  OAuthProvider,
  TokenExchangeRequest,
  TokenRefreshRequest,
} from './types'

const ACCESS_TOKEN_TTL_SEC = 15 * 60
const REFRESH_TOKEN_TTL_MS = 30 * 24 * 60 * 60 * 1000
const OAUTH_STATE_TTL_SEC = 10 * 60
const OAUTH_CODE_TTL_MS = 2 * 60 * 1000

const DEFAULT_GOOGLE_AUTH_URL = 'https://accounts.google.com/o/oauth2/v2/auth'
const DEFAULT_GOOGLE_TOKEN_URL = 'https://oauth2.googleapis.com/token'
const DEFAULT_GOOGLE_JWKS_URL = 'https://www.googleapis.com/oauth2/v3/certs'

const DEFAULT_APPLE_AUTH_URL = 'https://appleid.apple.com/auth/authorize'
const DEFAULT_APPLE_TOKEN_URL = 'https://appleid.apple.com/auth/token'
const DEFAULT_APPLE_JWKS_URL = 'https://appleid.apple.com/auth/keys'

const PKCE_CHALLENGE_RE = /^[A-Za-z0-9_-]{43,128}$/
const PKCE_VERIFIER_RE = /^[A-Za-z0-9._~-]{43,128}$/
const TEXT_ENCODER = new TextEncoder()

export function isGoogleConfigured(env: Env): boolean {
  return Boolean(
    env.DB &&
      env.JWT_SECRET &&
      env.GOOGLE_CLIENT_ID &&
      env.GOOGLE_CLIENT_SECRET,
  )
}

export function isAppleConfigured(env: Env): boolean {
  return Boolean(
    env.DB &&
      env.JWT_SECRET &&
      env.APPLE_CLIENT_ID &&
      env.APPLE_TEAM_ID &&
      env.APPLE_KEY_ID &&
      env.APPLE_PRIVATE_KEY,
  )
}

export function getAuthProviders(env: Env): AuthProvidersResponse {
  return {
    google: isGoogleConfigured(env),
    apple: isAppleConfigured(env),
    devToken: Boolean(env.DEV_AUTH_TOKEN && env.DEV_AUTH_TOKEN.length > 0),
  }
}

export function isAllowedClientRedirectUri(raw: string): boolean {
  if (raw === 'rem://auth/callback') return true
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    return false
  }
  if (url.protocol !== 'http:') return false
  if (url.hostname !== '127.0.0.1' && url.hostname !== 'localhost') return false
  if (url.pathname !== '/auth/callback') return false
  if (url.username || url.password || url.search || url.hash) return false
  const port = Number.parseInt(url.port, 10)
  return Number.isInteger(port) && port >= 1024 && port <= 65535
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function redirectToClient(
  redirectUri: string,
  params: Record<string, string | undefined>,
): Response {
  const url = new URL(redirectUri)
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v.length > 0) {
      url.searchParams.set(k, v)
    }
  }
  return new Response(null, {
    status: 302,
    headers: {
      Location: url.toString(),
      'Cache-Control': 'no-store',
    },
  })
}

function parseProvider(raw: string): OAuthProvider | null {
  if (raw === 'google' || raw === 'apple') return raw
  return null
}

function serverCallbackUrl(requestUrl: URL, provider: OAuthProvider): string {
  return `${requestUrl.origin}/v1/auth/${provider}/callback`
}

export async function handleOAuthStart(
  request: Request,
  env: Env,
  rawProvider: string,
): Promise<Response> {
  const provider = parseProvider(rawProvider)
  if (!provider) {
    return jsonResponse({ error: 'unsupported_provider' }, 404)
  }
  if (provider === 'google' && !isGoogleConfigured(env)) {
    return jsonResponse({ error: 'provider_not_configured' }, 503)
  }
  if (provider === 'apple' && !isAppleConfigured(env)) {
    return jsonResponse({ error: 'provider_not_configured' }, 503)
  }

  const url = new URL(request.url)
  const redirectUri = url.searchParams.get('redirect_uri') ?? ''
  const codeChallenge = url.searchParams.get('code_challenge') ?? ''
  const method = url.searchParams.get('code_challenge_method') ?? 'S256'
  const clientState = url.searchParams.get('state') ?? undefined

  if (!isAllowedClientRedirectUri(redirectUri)) {
    return jsonResponse({ error: 'invalid_redirect_uri' }, 400)
  }
  if (method !== 'S256' || !PKCE_CHALLENGE_RE.test(codeChallenge)) {
    return jsonResponse({ error: 'invalid_pkce_challenge' }, 400)
  }
  if (clientState !== undefined && clientState.length > 256) {
    return jsonResponse({ error: 'invalid_state' }, 400)
  }

  const nonce = randomToken(16)
  const nowSec = Math.floor(Date.now() / 1000)
  const stateToken = await signHs256Jwt(
    {
      typ: 'oauth_state',
      provider,
      redirectUri,
      codeChallenge,
      nonce,
      clientState,
      exp: nowSec + OAUTH_STATE_TTL_SEC,
    },
    env.JWT_SECRET!,
  )

  const callbackUrl = serverCallbackUrl(url, provider)
  if (provider === 'google') {
    const authUrl = new URL(env.GOOGLE_AUTH_URL ?? DEFAULT_GOOGLE_AUTH_URL)
    authUrl.searchParams.set('client_id', env.GOOGLE_CLIENT_ID!)
    authUrl.searchParams.set('redirect_uri', callbackUrl)
    authUrl.searchParams.set('response_type', 'code')
    authUrl.searchParams.set('scope', 'openid email')
    authUrl.searchParams.set('state', stateToken)
    authUrl.searchParams.set('nonce', nonce)
    authUrl.searchParams.set('prompt', 'select_account')
    return new Response(null, {
      status: 302,
      headers: { Location: authUrl.toString(), 'Cache-Control': 'no-store' },
    })
  }

  const authUrl = new URL(env.APPLE_AUTH_URL ?? DEFAULT_APPLE_AUTH_URL)
  authUrl.searchParams.set('client_id', env.APPLE_CLIENT_ID!)
  authUrl.searchParams.set('redirect_uri', callbackUrl)
  authUrl.searchParams.set('response_type', 'code')
  authUrl.searchParams.set('response_mode', 'form_post')
  authUrl.searchParams.set('scope', 'name email')
  authUrl.searchParams.set('state', stateToken)
  authUrl.searchParams.set('nonce', nonce)
  return new Response(null, {
    status: 302,
    headers: { Location: authUrl.toString(), 'Cache-Control': 'no-store' },
  })
}

export async function handleOAuthCallback(
  request: Request,
  env: Env,
  rawProvider: string,
): Promise<Response> {
  const provider = parseProvider(rawProvider)
  if (!provider) {
    return jsonResponse({ error: 'unsupported_provider' }, 404)
  }
  if (!env.DB || !env.JWT_SECRET) {
    return jsonResponse({ error: 'auth_not_configured' }, 503)
  }

  const url = new URL(request.url)
  let code = ''
  let state = ''
  let idpError = ''
  let appleUserEmail: string | null = null

  if (request.method === 'POST') {
    const form = await request.formData()
    code = String(form.get('code') ?? '')
    state = String(form.get('state') ?? '')
    idpError = String(form.get('error') ?? '')
    const userJson = form.get('user')
    if (typeof userJson === 'string' && userJson.length > 0) {
      try {
        const parsed = JSON.parse(userJson) as { email?: unknown }
        if (typeof parsed.email === 'string' && parsed.email.length > 0) {
          appleUserEmail = parsed.email
        }
      } catch {
        // Ignore malformed optional Apple user payload.
      }
    }
  } else {
    code = url.searchParams.get('code') ?? ''
    state = url.searchParams.get('state') ?? ''
    idpError = url.searchParams.get('error') ?? ''
  }

  const statePayload = state ? await verifyHs256Jwt(state, env.JWT_SECRET) : null
  if (
    !statePayload ||
    statePayload.typ !== 'oauth_state' ||
    statePayload.provider !== provider ||
    typeof statePayload.redirectUri !== 'string' ||
    !isAllowedClientRedirectUri(statePayload.redirectUri) ||
    typeof statePayload.codeChallenge !== 'string' ||
    typeof statePayload.nonce !== 'string'
  ) {
    return jsonResponse({ error: 'invalid_oauth_state' }, 400)
  }

  const clientRedirectUri = statePayload.redirectUri
  const clientState =
    typeof statePayload.clientState === 'string' ? statePayload.clientState : undefined

  if (idpError) {
    return redirectToClient(clientRedirectUri, { error: idpError, state: clientState })
  }
  if (!code) {
    return redirectToClient(clientRedirectUri, { error: 'missing_code', state: clientState })
  }

  const callbackUrl = serverCallbackUrl(url, provider)
  let identity: { sub: string; email: string | null }
  try {
    identity =
      provider === 'google'
        ? await exchangeGoogleCode(env, code, callbackUrl, statePayload.nonce)
        : await exchangeAppleCode(env, code, callbackUrl, statePayload.nonce, appleUserEmail)
  } catch (err) {
    const reason = err instanceof Error ? err.message : 'oauth_exchange_failed'
    return redirectToClient(clientRedirectUri, {
      error: 'oauth_exchange_failed',
      error_description: reason,
      state: clientState,
    })
  }

  const now = Date.now()
  const userId = await upsertOAuthUser(env.DB, provider, identity.sub, identity.email, now)

  const oneTimeCode = `rem_code_${randomToken(32)}`
  const codeHash = await sha256HexString(oneTimeCode)
  await env.DB.prepare(
    `INSERT INTO oauth_codes(
       code_hash, user_id, provider, email, code_challenge, redirect_uri, expires_at, created_at
     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)`,
  )
    .bind(
      codeHash,
      userId,
      provider,
      identity.email,
      statePayload.codeChallenge,
      clientRedirectUri,
      now + OAUTH_CODE_TTL_MS,
      now,
    )
    .run()

  return redirectToClient(clientRedirectUri, { code: oneTimeCode, state: clientState })
}

export async function handleTokenExchange(request: Request, env: Env): Promise<Response> {
  if (!env.DB || !env.JWT_SECRET) {
    return jsonResponse({ error: 'auth_not_configured' }, 503)
  }

  let body: TokenExchangeRequest
  try {
    body = (await request.json()) as TokenExchangeRequest
  } catch {
    return jsonResponse({ error: 'invalid_json' }, 400)
  }

  if (
    !body ||
    typeof body.code !== 'string' ||
    body.code.length === 0 ||
    typeof body.codeVerifier !== 'string' ||
    !PKCE_VERIFIER_RE.test(body.codeVerifier) ||
    typeof body.redirectUri !== 'string' ||
    !isAllowedClientRedirectUri(body.redirectUri)
  ) {
    return jsonResponse({ error: 'invalid_request' }, 400)
  }

  const codeHash = await sha256HexString(body.code)
  const row = await env.DB.prepare(
    `SELECT code_hash, user_id, provider, email, code_challenge, redirect_uri, expires_at
     FROM oauth_codes WHERE code_hash = ?1`,
  )
    .bind(codeHash)
    .first<{
      code_hash: string
      user_id: string
      provider: string
      email: string | null
      code_challenge: string
      redirect_uri: string
      expires_at: number
    }>()

  if (!row) {
    return jsonResponse({ error: 'invalid_code' }, 401)
  }

  const deleted = await env.DB.prepare('DELETE FROM oauth_codes WHERE code_hash = ?1')
    .bind(codeHash)
    .run()
  if ((deleted.meta.changes ?? 0) !== 1) {
    return jsonResponse({ error: 'invalid_code' }, 401)
  }

  const now = Date.now()
  if (row.expires_at <= now) {
    return jsonResponse({ error: 'code_expired' }, 401)
  }
  if (row.redirect_uri !== body.redirectUri) {
    return jsonResponse({ error: 'redirect_uri_mismatch' }, 401)
  }

  const computedChallenge = await pkceS256Challenge(body.codeVerifier)
  if (!timingSafeStringEqual(computedChallenge, row.code_challenge)) {
    return jsonResponse({ error: 'invalid_pkce_verifier' }, 401)
  }

  const deviceId = normalizeDeviceId(body.deviceId)
  const familyId = `fam_${randomToken(16)}`
  const tokens = await issueSessionTokens(
    env.DB,
    env.JWT_SECRET,
    row.user_id,
    familyId,
    deviceId,
    now,
  )

  const response: AuthTokenResponse = {
    accessToken: tokens.accessToken,
    expiresIn: ACCESS_TOKEN_TTL_SEC,
    refreshToken: tokens.refreshToken,
    account: {
      id: row.user_id,
      email: row.email,
      provider: row.provider,
    },
  }
  return jsonResponse(response)
}

export async function handleTokenRefresh(request: Request, env: Env): Promise<Response> {
  if (!env.DB || !env.JWT_SECRET) {
    return jsonResponse({ error: 'auth_not_configured' }, 503)
  }

  let body: TokenRefreshRequest
  try {
    body = (await request.json()) as TokenRefreshRequest
  } catch {
    return jsonResponse({ error: 'invalid_json' }, 400)
  }
  if (!body || typeof body.refreshToken !== 'string' || body.refreshToken.length === 0) {
    return jsonResponse({ error: 'invalid_request' }, 400)
  }

  const tokenHash = await sha256HexString(body.refreshToken)
  const row = await env.DB.prepare(
    `SELECT token_hash, family_id, user_id, device_id, expires_at, revoked_at
     FROM refresh_tokens WHERE token_hash = ?1`,
  )
    .bind(tokenHash)
    .first<{
      token_hash: string
      family_id: string
      user_id: string
      device_id: string
      expires_at: number
      revoked_at: number | null
    }>()

  if (!row) {
    return jsonResponse({ error: 'invalid_refresh_token' }, 401)
  }

  const now = Date.now()
  if (row.revoked_at !== null) {
    // Refresh token reuse detected: revoke the entire token family immediately.
    await env.DB.prepare(
      'UPDATE refresh_tokens SET revoked_at = ?1 WHERE family_id = ?2 AND revoked_at IS NULL',
    )
      .bind(now, row.family_id)
      .run()
    return jsonResponse({ error: 'refresh_token_reused' }, 401)
  }

  if (row.expires_at <= now) {
    return jsonResponse({ error: 'refresh_token_expired' }, 401)
  }

  const revoked = await env.DB.prepare(
    'UPDATE refresh_tokens SET revoked_at = ?1 WHERE token_hash = ?2 AND revoked_at IS NULL',
  )
    .bind(now, tokenHash)
    .run()
  if ((revoked.meta.changes ?? 0) !== 1) {
    await env.DB.prepare(
      'UPDATE refresh_tokens SET revoked_at = ?1 WHERE family_id = ?2 AND revoked_at IS NULL',
    )
      .bind(now, row.family_id)
      .run()
    return jsonResponse({ error: 'refresh_token_reused' }, 401)
  }

  const user = await env.DB.prepare('SELECT id, email FROM users WHERE id = ?1')
    .bind(row.user_id)
    .first<{ id: string; email: string | null }>()
  if (!user) {
    return jsonResponse({ error: 'account_not_found' }, 401)
  }
  const identity = await env.DB.prepare(
    'SELECT provider FROM auth_identities WHERE user_id = ?1 ORDER BY created_at ASC LIMIT 1',
  )
    .bind(row.user_id)
    .first<{ provider: string }>()

  const deviceId = body.deviceId ? normalizeDeviceId(body.deviceId) : row.device_id
  const tokens = await issueSessionTokens(
    env.DB,
    env.JWT_SECRET,
    row.user_id,
    row.family_id,
    deviceId,
    now,
  )

  const response: AuthTokenResponse = {
    accessToken: tokens.accessToken,
    expiresIn: ACCESS_TOKEN_TTL_SEC,
    refreshToken: tokens.refreshToken,
    account: {
      id: user.id,
      email: user.email,
      provider: identity?.provider ?? 'unknown',
    },
  }
  return jsonResponse(response)
}

export async function handleLogout(request: Request, env: Env): Promise<Response> {
  let body: LogoutRequest
  try {
    body = (await request.json()) as LogoutRequest
  } catch {
    return jsonResponse({ error: 'invalid_json' }, 400)
  }
  if (!body || typeof body.refreshToken !== 'string' || body.refreshToken.length === 0) {
    return jsonResponse({ error: 'invalid_request' }, 400)
  }

  if (env.DB) {
    const tokenHash = await sha256HexString(body.refreshToken)
    const row = await env.DB.prepare('SELECT family_id FROM refresh_tokens WHERE token_hash = ?1')
      .bind(tokenHash)
      .first<{ family_id: string }>()
    if (row) {
      await env.DB.prepare(
        'UPDATE refresh_tokens SET revoked_at = ?1 WHERE family_id = ?2 AND revoked_at IS NULL',
      )
        .bind(Date.now(), row.family_id)
        .run()
    }
  }

  return jsonResponse({ ok: true })
}

export async function getAccountInfo(
  env: Env,
  userId: string,
  authMethod: 'jwt' | 'dev_token',
): Promise<AccountInfo> {
  if (authMethod === 'dev_token' || !env.DB) {
    return {
      id: userId,
      email: null,
      providers: authMethod === 'dev_token' ? ['dev_token'] : [],
      authMethod,
    }
  }

  const user = await env.DB.prepare('SELECT id, email FROM users WHERE id = ?1')
    .bind(userId)
    .first<{ id: string; email: string | null }>()
  const identities = await env.DB.prepare(
    'SELECT provider FROM auth_identities WHERE user_id = ?1 ORDER BY created_at ASC',
  )
    .bind(userId)
    .all<{ provider: string }>()

  return {
    id: user?.id ?? userId,
    email: user?.email ?? null,
    providers: identities.results.map((r) => r.provider),
    authMethod,
  }
}

export async function deleteAccountData(env: Env, userId: string): Promise<void> {
  // 1. Delete all user assets in R2.
  const prefix = `users/${userId}/assets/`
  let cursor: string | undefined
  do {
    const listed = await env.ASSETS.list({ prefix, cursor, limit: 1000 })
    if (listed.objects.length > 0) {
      await env.ASSETS.delete(listed.objects.map((o) => o.key))
    }
    cursor = listed.truncated ? listed.cursor : undefined
  } while (cursor)

  // 2. Delete all SQLite data in the user's Durable Object.
  const stub = env.USER_SYNC.get(env.USER_SYNC.idFromName(userId))
  await stub.deleteAllData()

  // 3. Delete all auth/account rows in D1.
  if (env.DB) {
    await env.DB.batch([
      env.DB.prepare('DELETE FROM oauth_codes WHERE user_id = ?1').bind(userId),
      env.DB.prepare('DELETE FROM refresh_tokens WHERE user_id = ?1').bind(userId),
      env.DB.prepare('DELETE FROM auth_identities WHERE user_id = ?1').bind(userId),
      env.DB.prepare('DELETE FROM users WHERE id = ?1').bind(userId),
    ])
  }
}

async function issueSessionTokens(
  db: D1Database,
  jwtSecret: string,
  userId: string,
  familyId: string,
  deviceId: string,
  nowMs: number,
): Promise<{ accessToken: string; refreshToken: string }> {
  const nowSec = Math.floor(nowMs / 1000)
  const accessToken = await signHs256Jwt(
    {
      sub: userId,
      typ: 'access',
      iat: nowSec,
      exp: nowSec + ACCESS_TOKEN_TTL_SEC,
    },
    jwtSecret,
  )

  const refreshToken = `rem_rt_${randomToken(32)}`
  const tokenHash = await sha256HexString(refreshToken)
  await db
    .prepare(
      `INSERT INTO refresh_tokens(
         token_hash, family_id, user_id, device_id, expires_at, revoked_at, created_at
       ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)`,
    )
    .bind(tokenHash, familyId, userId, deviceId, nowMs + REFRESH_TOKEN_TTL_MS, nowMs)
    .run()

  return { accessToken, refreshToken }
}

async function upsertOAuthUser(
  db: D1Database,
  provider: OAuthProvider,
  providerSub: string,
  email: string | null,
  now: number,
): Promise<string> {
  const existing = await db
    .prepare('SELECT user_id FROM auth_identities WHERE provider = ?1 AND provider_sub = ?2')
    .bind(provider, providerSub)
    .first<{ user_id: string }>()

  if (existing) {
    if (email) {
      await db.batch([
        db
          .prepare('UPDATE auth_identities SET email = ?1 WHERE provider = ?2 AND provider_sub = ?3')
          .bind(email, provider, providerSub),
        db
          .prepare('UPDATE users SET email = COALESCE(email, ?1) WHERE id = ?2')
          .bind(email, existing.user_id),
      ])
    }
    return existing.user_id
  }

  const userId = `usr_${randomToken(16)}`
  await db.batch([
    db
      .prepare('INSERT INTO users(id, email, created_at) VALUES (?1, ?2, ?3)')
      .bind(userId, email, now),
    db
      .prepare(
        `INSERT INTO auth_identities(provider, provider_sub, user_id, email, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)`,
      )
      .bind(provider, providerSub, userId, email, now),
  ])
  return userId
}

function normalizeDeviceId(raw: string | undefined): string {
  const trimmed = raw?.trim() ?? ''
  if (trimmed.length === 0) return 'unknown'
  return trimmed.slice(0, 64)
}

async function exchangeGoogleCode(
  env: Env,
  code: string,
  callbackUrl: string,
  expectedNonce: string,
): Promise<{ sub: string; email: string | null }> {
  const form = new URLSearchParams({
    code,
    client_id: env.GOOGLE_CLIENT_ID!,
    client_secret: env.GOOGLE_CLIENT_SECRET!,
    redirect_uri: callbackUrl,
    grant_type: 'authorization_code',
  })
  const resp = await fetch(env.GOOGLE_TOKEN_URL ?? DEFAULT_GOOGLE_TOKEN_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: form.toString(),
  })
  if (!resp.ok) {
    throw new Error(`google token endpoint returned ${resp.status}`)
  }
  const data = (await resp.json()) as { id_token?: unknown }
  if (typeof data.id_token !== 'string') {
    throw new Error('google token response missing id_token')
  }

  const claims = await verifyOidcIdToken({
    idToken: data.id_token,
    jwksUrl: env.GOOGLE_JWKS_URL ?? DEFAULT_GOOGLE_JWKS_URL,
    allowedIssuers: ['https://accounts.google.com', 'accounts.google.com'],
    expectedAudience: env.GOOGLE_CLIENT_ID!,
    expectedNonce,
  })
  const email = typeof claims.email === 'string' && claims.email.length > 0 ? claims.email : null
  return { sub: String(claims.sub), email }
}

async function exchangeAppleCode(
  env: Env,
  code: string,
  callbackUrl: string,
  expectedNonce: string,
  fallbackEmail: string | null,
): Promise<{ sub: string; email: string | null }> {
  const clientSecret = await signAppleClientSecret(env)
  const form = new URLSearchParams({
    code,
    client_id: env.APPLE_CLIENT_ID!,
    client_secret: clientSecret,
    redirect_uri: callbackUrl,
    grant_type: 'authorization_code',
  })
  const resp = await fetch(env.APPLE_TOKEN_URL ?? DEFAULT_APPLE_TOKEN_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: form.toString(),
  })
  if (!resp.ok) {
    throw new Error(`apple token endpoint returned ${resp.status}`)
  }
  const data = (await resp.json()) as { id_token?: unknown }
  if (typeof data.id_token !== 'string') {
    throw new Error('apple token response missing id_token')
  }

  const claims = await verifyOidcIdToken({
    idToken: data.id_token,
    jwksUrl: env.APPLE_JWKS_URL ?? DEFAULT_APPLE_JWKS_URL,
    allowedIssuers: ['https://appleid.apple.com'],
    expectedAudience: env.APPLE_CLIENT_ID!,
    expectedNonce,
  })
  const email =
    typeof claims.email === 'string' && claims.email.length > 0 ? claims.email : fallbackEmail
  return { sub: String(claims.sub), email }
}

async function signAppleClientSecret(env: Env): Promise<string> {
  const nowSec = Math.floor(Date.now() / 1000)
  const headerB64 = encodeBase64Url(
    TEXT_ENCODER.encode(JSON.stringify({ alg: 'ES256', kid: env.APPLE_KEY_ID! })),
  )
  const payloadB64 = encodeBase64Url(
    TEXT_ENCODER.encode(
      JSON.stringify({
        iss: env.APPLE_TEAM_ID!,
        iat: nowSec,
        exp: nowSec + 300,
        aud: 'https://appleid.apple.com',
        sub: env.APPLE_CLIENT_ID!,
      }),
    ),
  )

  const pkcs8 = pemToDer(env.APPLE_PRIVATE_KEY!)
  const key = await crypto.subtle.importKey(
    'pkcs8',
    pkcs8,
    { name: 'ECDSA', namedCurve: 'P-256' },
    false,
    ['sign'],
  )
  const signingInput = TEXT_ENCODER.encode(`${headerB64}.${payloadB64}`)
  const sig = new Uint8Array(
    await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, key, signingInput),
  )
  return `${headerB64}.${payloadB64}.${encodeBase64Url(sig)}`
}

function pemToDer(pem: string): ArrayBuffer {
  const normalized = pem
    .replace(/\\n/g, '\n')
    .replace(/-----BEGIN [^-]+-----/g, '')
    .replace(/-----END [^-]+-----/g, '')
    .replace(/\s+/g, '')
  const binary = atob(normalized)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  return bytes.buffer
}

interface JwkKey extends JsonWebKey {
  kid?: string
  alg?: string
}

async function verifyOidcIdToken(opts: {
  idToken: string
  jwksUrl: string
  allowedIssuers: string[]
  expectedAudience: string
  expectedNonce: string
}): Promise<Record<string, unknown>> {
  const parts = opts.idToken.split('.')
  if (parts.length !== 3) throw new Error('malformed id_token')
  const [headerB64, payloadB64, sigB64] = parts

  let header: { alg?: unknown; kid?: unknown }
  let payload: Record<string, unknown>
  let signatureBytes: Uint8Array
  try {
    header = JSON.parse(decodeBase64UrlToString(headerB64)) as { alg?: unknown; kid?: unknown }
    payload = JSON.parse(decodeBase64UrlToString(payloadB64)) as Record<string, unknown>
    signatureBytes = decodeBase64Url(sigB64)
  } catch {
    throw new Error('malformed id_token encoding')
  }

  if (header.alg !== 'RS256' && header.alg !== 'ES256') {
    throw new Error(`unsupported id_token alg: ${String(header.alg)}`)
  }
  const jwksResp = await fetch(opts.jwksUrl)
  if (!jwksResp.ok) {
    throw new Error(`jwks fetch failed: ${jwksResp.status}`)
  }
  const jwks = (await jwksResp.json()) as { keys?: JwkKey[] }
  const keys = Array.isArray(jwks.keys) ? jwks.keys : []
  const matched = keys.find(
    (k) => (typeof header.kid !== 'string' || k.kid === header.kid) && (!k.alg || k.alg === header.alg),
  )
  if (!matched) {
    throw new Error('no matching jwk for id_token')
  }

  const signingInput = TEXT_ENCODER.encode(`${headerB64}.${payloadB64}`)
  let valid = false
  if (header.alg === 'RS256') {
    const key = await crypto.subtle.importKey(
      'jwk',
      matched,
      { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' },
      false,
      ['verify'],
    )
    valid = await crypto.subtle.verify('RSASSA-PKCS1-v1_5', key, signatureBytes, signingInput)
  } else {
    const key = await crypto.subtle.importKey(
      'jwk',
      matched,
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['verify'],
    )
    valid = await crypto.subtle.verify(
      { name: 'ECDSA', hash: 'SHA-256' },
      key,
      signatureBytes,
      signingInput,
    )
  }
  if (!valid) {
    throw new Error('invalid id_token signature')
  }

  if (typeof payload.iss !== 'string' || !opts.allowedIssuers.includes(payload.iss)) {
    throw new Error('invalid id_token issuer')
  }
  const audValid =
    payload.aud === opts.expectedAudience ||
    (Array.isArray(payload.aud) && payload.aud.includes(opts.expectedAudience))
  if (!audValid) {
    throw new Error('invalid id_token audience')
  }
  const nowSec = Math.floor(Date.now() / 1000)
  if (typeof payload.exp !== 'number' || payload.exp <= nowSec) {
    throw new Error('expired id_token')
  }
  if (typeof payload.sub !== 'string' || payload.sub.length === 0) {
    throw new Error('id_token missing sub')
  }
  if (typeof payload.nonce !== 'string' || !timingSafeStringEqual(payload.nonce, opts.expectedNonce)) {
    throw new Error('id_token nonce mismatch')
  }

  return payload
}
