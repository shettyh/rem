import { env } from 'cloudflare:test'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import migrationSql from '../migrations/0001_auth.sql?raw'
import entitlementsMigrationSql from '../migrations/0002_entitlements.sql?raw'
import earlyAccessMigrationSql from '../migrations/0003_early_access.sql?raw'
import trialInvitesBillingMigrationSql from '../migrations/0004_trial_invites_billing.sql?raw'
import worker from '../src/index'
import type { Env } from '../src/types'

const TEST_ENV = env as unknown as Env

function toBase64Url(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i])
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

async function pkceChallenge(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))
  return toBase64Url(new Uint8Array(digest))
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

function derToPem(der: ArrayBuffer, label: string): string {
  const bytes = new Uint8Array(der)
  let binary = ''
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i])
  const b64 = btoa(binary).match(/.{1,64}/g)?.join('\n') ?? ''
  return `-----BEGIN ${label}-----\n${b64}\n-----END ${label}-----`
}

async function generateRs256KeyPair() {
  const pair = (await crypto.subtle.generateKey(
    {
      name: 'RSASSA-PKCS1-v1_5',
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: 'SHA-256',
    },
    true,
    ['sign', 'verify'],
  )) as CryptoKeyPair
  const publicJwk = (await crypto.subtle.exportKey('jwk', pair.publicKey)) as JsonWebKey
  return { privateKey: pair.privateKey, publicJwk: { ...publicJwk, kid: 'test-rsa-1', alg: 'RS256' } }
}

async function generateEs256KeyPair() {
  const pair = (await crypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' },
    true,
    ['sign', 'verify'],
  )) as CryptoKeyPair
  const publicJwk = (await crypto.subtle.exportKey('jwk', pair.publicKey)) as JsonWebKey
  const pkcs8 = (await crypto.subtle.exportKey('pkcs8', pair.privateKey)) as ArrayBuffer
  return {
    privateKey: pair.privateKey,
    privatePem: derToPem(pkcs8, 'PRIVATE KEY'),
    publicJwk: { ...publicJwk, kid: 'test-ec-1', alg: 'ES256' },
  }
}

async function signRs256Jwt(
  payload: Record<string, unknown>,
  privateKey: CryptoKey,
  kid = 'test-rsa-1',
): Promise<string> {
  const enc = new TextEncoder()
  const headerB64 = toBase64Url(enc.encode(JSON.stringify({ alg: 'RS256', typ: 'JWT', kid })))
  const payloadB64 = toBase64Url(enc.encode(JSON.stringify(payload)))
  const sig = new Uint8Array(
    await crypto.subtle.sign('RSASSA-PKCS1-v1_5', privateKey, enc.encode(`${headerB64}.${payloadB64}`)),
  )
  return `${headerB64}.${payloadB64}.${toBase64Url(sig)}`
}

async function signEs256Jwt(
  payload: Record<string, unknown>,
  privateKey: CryptoKey,
  kid = 'test-ec-1',
): Promise<string> {
  const enc = new TextEncoder()
  const headerB64 = toBase64Url(enc.encode(JSON.stringify({ alg: 'ES256', typ: 'JWT', kid })))
  const payloadB64 = toBase64Url(enc.encode(JSON.stringify(payload)))
  const sig = new Uint8Array(
    await crypto.subtle.sign(
      { name: 'ECDSA', hash: 'SHA-256' },
      privateKey,
      enc.encode(`${headerB64}.${payloadB64}`),
    ),
  )
  return `${headerB64}.${payloadB64}.${toBase64Url(sig)}`
}

async function applyAuthMigration() {
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

function callWorker(url: string, init?: RequestInit, customEnv: Env = TEST_ENV): Promise<Response> {
  return worker.fetch(new Request(url, init), customEnv)
}

describe('OAuth, token refresh, isolation, and account deletion', () => {
  let googleKey: Awaited<ReturnType<typeof generateRs256KeyPair>>
  let appleClientKey: Awaited<ReturnType<typeof generateEs256KeyPair>>
  let appleIdpKey: Awaited<ReturnType<typeof generateEs256KeyPair>>

  beforeAll(async () => {
    await applyAuthMigration()
    googleKey = await generateRs256KeyPair()
    appleClientKey = await generateEs256KeyPair()
    appleIdpKey = await generateEs256KeyPair()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  async function completeGoogleLogin(opts: {
    sub: string
    email: string
    redirectUri?: string
    deviceId?: string
  }) {
    const redirectUri = opts.redirectUri ?? 'http://127.0.0.1:41234/auth/callback'
    const verifier = 'google_pkce_verifier_0123456789abcdefghijklmnopqrstuvwxyz'
    const challenge = await pkceChallenge(verifier)

    const startUrl = new URL('https://sync.example.com/v1/auth/google/start')
    startUrl.searchParams.set('redirect_uri', redirectUri)
    startUrl.searchParams.set('code_challenge', challenge)
    startUrl.searchParams.set('code_challenge_method', 'S256')
    startUrl.searchParams.set('state', 'client-csrf-123')

    const startResp = await callWorker(startUrl.toString())
    expect(startResp.status).toBe(302)
    const idpUrl = new URL(startResp.headers.get('Location')!)
    expect(idpUrl.origin).toBe('https://accounts.google.com')
    const serverState = idpUrl.searchParams.get('state')!
    const nonce = idpUrl.searchParams.get('nonce')!

    const nowSec = Math.floor(Date.now() / 1000)
    const idToken = await signRs256Jwt(
      {
        iss: 'https://accounts.google.com',
        aud: 'google-test-client-id',
        sub: opts.sub,
        email: opts.email,
        nonce,
        iat: nowSec,
        exp: nowSec + 300,
      },
      googleKey.privateKey,
    )

    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const target = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url
      if (target === 'https://oauth2.googleapis.com/token') {
        return new Response(JSON.stringify({ id_token: idToken }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      }
      if (target === 'https://www.googleapis.com/oauth2/v3/certs') {
        return new Response(JSON.stringify({ keys: [googleKey.publicJwk] }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      }
      throw new Error(`unexpected outbound fetch: ${target}`)
    })

    const cbUrl = new URL('https://sync.example.com/v1/auth/google/callback')
    cbUrl.searchParams.set('code', 'idp-auth-code-1')
    cbUrl.searchParams.set('state', serverState)
    const cbResp = await callWorker(cbUrl.toString())
    expect(cbResp.status).toBe(302)
    const clientCallback = new URL(cbResp.headers.get('Location')!)
    expect(clientCallback.origin + clientCallback.pathname).toBe(redirectUri)
    expect(clientCallback.searchParams.get('state')).toBe('client-csrf-123')
    const oneTimeCode = clientCallback.searchParams.get('code')!
    expect(oneTimeCode).toMatch(/^rem_code_/)

    const exchangeResp = await callWorker('https://sync.example.com/v1/auth/exchange', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        code: oneTimeCode,
        codeVerifier: verifier,
        redirectUri,
        deviceId: opts.deviceId ?? 'dev-mac-1',
      }),
    })
    expect(exchangeResp.status).toBe(200)
    return (await exchangeResp.json()) as {
      accessToken: string
      expiresIn: number
      refreshToken: string
      account: { id: string; email: string | null; provider: string }
    }
  }

  it('reports configured auth providers and rejects disallowed redirect URIs', async () => {
    const providers = await callWorker('https://sync.example.com/v1/auth/providers')
    expect(providers.status).toBe(200)
    expect(await providers.json()).toEqual({
      google: true,
      apple: false,
      devToken: true,
      subscriptionRequired: false,
      upgradeUrl: null,
    })

    const challenge = await pkceChallenge('verifier_0123456789abcdefghijklmnopqrstuvwxyz_12345')
    const evilRedirect = await callWorker(
      `https://sync.example.com/v1/auth/google/start?redirect_uri=${encodeURIComponent('https://evil.example.com/auth/callback')}&code_challenge=${challenge}`,
    )
    expect(evilRedirect.status).toBe(400)
  })

  it('completes Google OAuth + PKCE, rotates refresh tokens, detects reuse, and supports logout', async () => {
    const session = await completeGoogleLogin({
      sub: 'google-sub-alice',
      email: 'alice@example.com',
    })
    expect(session.account.id).toMatch(/^usr_/)
    expect(session.account.email).toBe('alice@example.com')
    expect(session.account.provider).toBe('google')
    expect(session.expiresIn).toBe(900)

    const accountResp = await callWorker('https://sync.example.com/v1/account', {
      headers: { Authorization: `Bearer ${session.accessToken}` },
    })
    expect(accountResp.status).toBe(200)
    expect(await accountResp.json()).toEqual({
      id: session.account.id,
      email: 'alice@example.com',
      providers: ['google'],
      authMethod: 'jwt',
      syncEnabled: true,
      subscriptionRequired: false,
      subscriptionStatus: 'none',
      upgradeUrl: null,
      entitlement: {
        state: 'self_hosted',
        syncEnabled: true,
        provider: 'self_hosted',
        plan: 'self_hosted',
        expiresAt: null,
        cancelAtPeriodEnd: false,
        canStartTrial: false,
        trialDays: 30,
        priceDisplay: '$5/year',
        canSubscribePolar: false,
        canManageBilling: false,
      },
    })

    const devAccountResp = await callWorker('https://sync.example.com/v1/account', {
      headers: { Authorization: 'Bearer test-dev-secret:selfhost-u1' },
    })
    expect(devAccountResp.status).toBe(200)
    expect(await devAccountResp.json()).toEqual({
      id: 'selfhost-u1',
      email: null,
      providers: ['dev_token'],
      authMethod: 'dev_token',
      syncEnabled: true,
      subscriptionRequired: false,
      subscriptionStatus: 'none',
      upgradeUrl: null,
      entitlement: {
        state: 'self_hosted',
        syncEnabled: true,
        provider: 'dev_token',
        plan: 'self_hosted',
        expiresAt: null,
        cancelAtPeriodEnd: false,
        canStartTrial: false,
        trialDays: 30,
        priceDisplay: '$5/year',
        canSubscribePolar: false,
        canManageBilling: false,
      },
    })

    // Refresh rotates the refresh token and issues a valid new access token.
    const refresh1 = await callWorker('https://sync.example.com/v1/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refreshToken: session.refreshToken, deviceId: 'dev-mac-1' }),
    })
    expect(refresh1.status).toBe(200)
    const refreshed = (await refresh1.json()) as typeof session
    expect(refreshed.refreshToken).not.toBe(session.refreshToken)
    expect(refreshed.account.id).toBe(session.account.id)

    // Reusing the OLD refresh token fails and revokes the entire token family.
    const reuse = await callWorker('https://sync.example.com/v1/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refreshToken: session.refreshToken }),
    })
    expect(reuse.status).toBe(401)
    expect(await reuse.json()).toEqual({ error: 'refresh_token_reused' })

    // Even the newly rotated token from the same family is now revoked.
    const afterReuse = await callWorker('https://sync.example.com/v1/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refreshToken: refreshed.refreshToken }),
    })
    expect(afterReuse.status).toBe(401)

    // Re-login maps to the exact same account ID, then logout revokes the new token.
    const session2 = await completeGoogleLogin({
      sub: 'google-sub-alice',
      email: 'alice@example.com',
    })
    expect(session2.account.id).toBe(session.account.id)

    const logoutResp = await callWorker('https://sync.example.com/v1/auth/logout', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refreshToken: session2.refreshToken }),
    })
    expect(logoutResp.status).toBe(200)

    const afterLogout = await callWorker('https://sync.example.com/v1/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refreshToken: session2.refreshToken }),
    })
    expect(afterLogout.status).toBe(401)
  })

  it('completes Sign in with Apple form_post flow with ES256 client secret and rem:// callback', async () => {
    const envWithApple: Env = {
      ...TEST_ENV,
      APPLE_PRIVATE_KEY: appleClientKey.privatePem,
    }
    const redirectUri = 'rem://auth/callback'
    const verifier = 'apple_pkce_verifier_0123456789abcdefghijklmnopqrstuvwxyz'
    const challenge = await pkceChallenge(verifier)

    const startResp = await callWorker(
      `https://sync.example.com/v1/auth/apple/start?redirect_uri=${encodeURIComponent(redirectUri)}&code_challenge=${challenge}`,
      undefined,
      envWithApple,
    )
    expect(startResp.status).toBe(302)
    const appleAuthUrl = new URL(startResp.headers.get('Location')!)
    expect(appleAuthUrl.origin).toBe('https://appleid.apple.com')
    expect(appleAuthUrl.searchParams.get('response_mode')).toBe('form_post')
    const serverState = appleAuthUrl.searchParams.get('state')!
    const nonce = appleAuthUrl.searchParams.get('nonce')!

    const nowSec = Math.floor(Date.now() / 1000)
    const appleIdToken = await signEs256Jwt(
      {
        iss: 'https://appleid.apple.com',
        aud: 'com.shettyh.rem.test',
        sub: 'apple-sub-001',
        email: 'appleuser@privaterelay.appleid.com',
        nonce,
        iat: nowSec,
        exp: nowSec + 300,
      },
      appleIdpKey.privateKey,
    )

    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const target = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url
      if (target === 'https://appleid.apple.com/auth/token') {
        return new Response(JSON.stringify({ id_token: appleIdToken }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      }
      if (target === 'https://appleid.apple.com/auth/keys') {
        return new Response(JSON.stringify({ keys: [appleIdpKey.publicJwk] }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      }
      throw new Error(`unexpected outbound fetch: ${target}`)
    })

    const form = new FormData()
    form.set('code', 'apple-code-xyz')
    form.set('state', serverState)
    const cbResp = await callWorker(
      'https://sync.example.com/v1/auth/apple/callback',
      { method: 'POST', body: form },
      envWithApple,
    )
    expect(cbResp.status).toBe(302)
    const location = cbResp.headers.get('Location')!
    expect(location.startsWith('rem://auth/callback?code=rem_code_')).toBe(true)
    const oneTimeCode = new URL(location).searchParams.get('code')!

    // Wrong PKCE verifier is rejected and consumes the one-time code.
    const badExchange = await callWorker(
      'https://sync.example.com/v1/auth/exchange',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          code: oneTimeCode,
          codeVerifier: 'wrong_verifier_0123456789abcdefghijklmnopqrstuvwxyz',
          redirectUri,
        }),
      },
      envWithApple,
    )
    expect(badExchange.status).toBe(401)
  })

  it('enforces two-user OAuth isolation and purges DO + R2 + D1 on DELETE /v1/account', async () => {
    const userA = await completeGoogleLogin({
      sub: 'google-iso-a',
      email: 'iso-a@example.com',
    })
    const userB = await completeGoogleLogin({
      sub: 'google-iso-b',
      email: 'iso-b@example.com',
    })
    expect(userA.account.id).not.toBe(userB.account.id)

    const bytes = new TextEncoder().encode('private-user-a-image')
    const hash = await sha256Hex(bytes)

    const putAsset = await callWorker(`https://sync.example.com/v1/assets/${hash}`, {
      method: 'PUT',
      headers: {
        Authorization: `Bearer ${userA.accessToken}`,
        'Content-Type': 'image/png',
      },
      body: bytes,
    })
    expect(putAsset.status).toBe(200)

    const syncA = await callWorker('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${userA.accessToken}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        deviceId: 'dev-a',
        sinceSeq: 0,
        ops: [
          {
            opId: 'op-iso-a-1',
            payload: { type: 'deckUpsert', id: 'd-a', hlc: '0000000001000-0000-aaaaaaaa' },
          },
        ],
      }),
    })
    expect(syncA.status).toBe(200)

    // User B sees neither User A's ops nor User A's asset.
    const syncB = await callWorker('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${userB.accessToken}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ deviceId: 'dev-b', sinceSeq: 0, ops: [] }),
    })
    expect(await syncB.json()).toEqual({
      latestSeq: 0,
      hasMore: false,
      ackedOpIds: [],
      ops: [],
    })
    const assetB = await callWorker(`https://sync.example.com/v1/assets/${hash}`, {
      headers: { Authorization: `Bearer ${userB.accessToken}` },
    })
    expect(assetB.status).toBe(404)

    // User A deletes their account.
    const delA = await callWorker('https://sync.example.com/v1/account', {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${userA.accessToken}` },
    })
    expect(delA.status).toBe(200)
    expect(await delA.json()).toEqual({ deleted: true })

    // User A's asset, DO ops, and refresh token are all gone.
    const assetAfterDel = await callWorker(`https://sync.example.com/v1/assets/${hash}`, {
      headers: { Authorization: `Bearer ${userA.accessToken}` },
    })
    expect(assetAfterDel.status).toBe(404)

    const refreshAfterDel = await callWorker('https://sync.example.com/v1/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refreshToken: userA.refreshToken }),
    })
    expect(refreshAfterDel.status).toBe(401)

    const opsAfterDel = await callWorker('https://sync.example.com/v1/sync', {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${userA.accessToken}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ deviceId: 'dev-a', sinceSeq: 0, ops: [] }),
    })
    expect(await opsAfterDel.json()).toEqual({
      latestSeq: 0,
      hasMore: false,
      ackedOpIds: [],
      ops: [],
    })
  })

  it('blocks unpaid users with 402 when REQUIRE_SUBSCRIPTION is true, while allowing allowlisted and subscribed users', async () => {
    const gatedEnv: Env = {
      ...TEST_ENV,
      REQUIRE_SUBSCRIPTION: 'true',
      ALLOWED_EMAILS: 'vip@example.com, owner@example.com',
      UPGRADE_URL: 'https://shettyh.fyi/rem-cloud',
    }

    const providers = await callWorker('https://sync.example.com/v1/auth/providers', undefined, gatedEnv)
    expect(await providers.json()).toEqual({
      google: true,
      apple: false,
      devToken: true,
      subscriptionRequired: true,
      upgradeUrl: 'https://shettyh.fyi/rem-cloud',
    })

    const unpaid = await completeGoogleLogin({
      sub: 'google-sub-unpaid',
      email: 'unpaid@example.com',
    })
    const allowlisted = await completeGoogleLogin({
      sub: 'google-sub-vip',
      email: 'VIP@example.com',
    })

    // Unpaid user can inspect their account, but sync & asset routes return 402.
    const unpaidAccount = await callWorker(
      'https://sync.example.com/v1/account',
      { headers: { Authorization: `Bearer ${unpaid.accessToken}` } },
      gatedEnv,
    )
    expect(unpaidAccount.status).toBe(200)
    expect(await unpaidAccount.json()).toMatchObject({
      id: unpaid.account.id,
      syncEnabled: false,
      subscriptionRequired: true,
      subscriptionStatus: 'none',
      upgradeUrl: 'https://shettyh.fyi/rem-cloud',
    })

    const blockedSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${unpaid.accessToken}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ deviceId: 'dev-u', sinceSeq: 0, ops: [] }),
      },
      gatedEnv,
    )
    expect(blockedSync.status).toBe(402)
    expect(await blockedSync.json()).toMatchObject({
      error: 'subscription_required',
      upgradeUrl: 'https://shettyh.fyi/rem-cloud',
    })

    const assetHash = await sha256Hex(new TextEncoder().encode('blocked-asset'))
    const blockedAsset = await callWorker(
      `https://sync.example.com/v1/assets/${assetHash}`,
      {
        method: 'PUT',
        headers: {
          Authorization: `Bearer ${unpaid.accessToken}`,
          'Content-Type': 'image/png',
        },
        body: 'blocked-asset',
      },
      gatedEnv,
    )
    expect(blockedAsset.status).toBe(402)

    // Allowlisted user can sync immediately.
    const vipSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${allowlisted.accessToken}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ deviceId: 'dev-vip', sinceSeq: 0, ops: [] }),
      },
      gatedEnv,
    )
    expect(vipSync.status).toBe(200)

    // Granting an active entitlement in D1 immediately unblocks the previously unpaid user.
    await TEST_ENV.DB!.prepare(
      `INSERT INTO user_entitlements(user_id, status, plan, provider, current_period_end, updated_at)
       VALUES (?1, 'active', 'cloud', 'manual', ?2, ?3)`,
    )
      .bind(unpaid.account.id, Date.now() + 86_400_000, Date.now())
      .run()

    const unblockedSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${unpaid.accessToken}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ deviceId: 'dev-u', sinceSeq: 0, ops: [] }),
      },
      gatedEnv,
    )
    expect(unblockedSync.status).toBe(200)
  })

  it('serves /premium and /cloud pages, retires /v1/waitlist with 410, and still honors legacy approved early_access rows', async () => {
    const gatedEnv: Env = {
      ...TEST_ENV,
      REQUIRE_SUBSCRIPTION: 'true',
      UPGRADE_URL: 'https://rem-sync.shettyh.fyi/premium',
    }

    for (const path of ['/premium', '/premium/', '/cloud', '/cloud/']) {
      const page = await callWorker(`https://sync.example.com${path}`, undefined, gatedEnv)
      expect(page.status).toBe(200)
      expect(page.headers.get('Content-Type')).toContain('text/html')
      const html = await page.text()
      expect(html).toContain('rem Premium')
      expect(html).toContain('/v1/trial/start')
      expect(html).toContain('/v1/invites/redeem')
    }

    const retiredWaitlist = await callWorker(
      'https://sync.example.com/v1/waitlist',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: 'beta@example.com' }),
      },
      gatedEnv,
    )
    expect(retiredWaitlist.status).toBe(410)
    expect(await retiredWaitlist.json()).toMatchObject({
      error: 'waitlist_retired',
      premiumUrl: 'https://sync.example.com/premium',
    })

    const betaUser = await completeGoogleLogin({
      sub: 'google-sub-beta',
      email: 'beta@example.com',
    })

    // Legacy approved row in early_access continues to grant indefinite invite access.
    await TEST_ENV.DB!.prepare(
      `INSERT INTO early_access(email, status, created_at, updated_at)
       VALUES ('beta@example.com', 'approved', ?1, ?1)
       ON CONFLICT(email) DO UPDATE SET status = 'approved', updated_at = ?1`,
    )
      .bind(Date.now())
      .run()

    const accountResp = await callWorker(
      'https://sync.example.com/v1/account',
      { headers: { Authorization: `Bearer ${betaUser.accessToken}` } },
      gatedEnv,
    )
    expect(accountResp.status).toBe(200)
    expect(await accountResp.json()).toMatchObject({
      email: 'beta@example.com',
      syncEnabled: true,
      subscriptionRequired: true,
      subscriptionStatus: 'allowlisted',
      entitlement: {
        state: 'invite',
        syncEnabled: true,
        provider: 'allowlist',
        plan: 'early_access',
        expiresAt: null,
        canStartTrial: false,
      },
    })

    const approvedSync = await callWorker(
      'https://sync.example.com/v1/sync',
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${betaUser.accessToken}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ deviceId: 'dev-beta', sinceSeq: 0, ops: [] }),
      },
      gatedEnv,
    )
    expect(approvedSync.status).toBe(200)
  })
})
