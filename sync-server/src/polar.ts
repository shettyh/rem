import { timingSafeStringEqual } from './auth'
import { checkSyncEntitlement, isPolarConfigured } from './entitlements'
import type { Env, PolarCheckoutResponse, PolarPortalResponse } from './types'

const DEFAULT_POLAR_API_BASE = 'https://api.polar.sh'
const WEBHOOK_TOLERANCE_SECONDS = 5 * 60
const TEXT_ENCODER = new TextEncoder()

const POLAR_SUBSCRIPTION_EVENTS = new Set([
  'subscription.created',
  'subscription.active',
  'subscription.updated',
  'subscription.canceled',
  'subscription.uncanceled',
  'subscription.revoked',
])

interface PolarSubscriptionPayload {
  id?: unknown
  status?: unknown
  customer_id?: unknown
  current_period_end?: unknown
  ends_at?: unknown
  ended_at?: unknown
  cancel_at_period_end?: unknown
  metadata?: Record<string, unknown> | null
  customer?: {
    id?: unknown
    external_id?: unknown
  } | null
}

interface PolarWebhookEnvelope {
  type?: unknown
  data?: PolarSubscriptionPayload | null
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

export function getPolarApiBase(env: Env): string {
  const raw = env.POLAR_API_BASE?.trim()
  const base = raw && raw.length > 0 ? raw : DEFAULT_POLAR_API_BASE
  let end = base.length
  while (end > 0 && base.charCodeAt(end - 1) === 47) {
    end -= 1
  }
  return base.slice(0, end)
}

function encodeBase64(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i])
  }
  return btoa(binary)
}

async function computeStandardWebhookSignature(
  secret: string,
  webhookId: string,
  webhookTimestamp: string,
  rawBody: string,
): Promise<string> {
  // Polar signs Standard Webhooks HMAC-SHA256 using the literal UTF-8 bytes of the configured secret.
  const key = await crypto.subtle.importKey(
    'raw',
    TEXT_ENCODER.encode(secret.trim()),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  )
  const signedContent = TEXT_ENCODER.encode(`${webhookId}.${webhookTimestamp}.${rawBody}`)
  const sig = new Uint8Array(await crypto.subtle.sign('HMAC', key, signedContent))
  return encodeBase64(sig)
}

export async function signPolarWebhook(
  secret: string,
  webhookId: string,
  webhookTimestamp: string,
  rawBody: string,
): Promise<string> {
  const b64 = await computeStandardWebhookSignature(secret, webhookId, webhookTimestamp, rawBody)
  return `v1,${b64}`
}

export async function verifyPolarWebhookSignature(
  secret: string,
  headers: Headers,
  rawBody: string,
  nowSec = Math.floor(Date.now() / 1000),
): Promise<{ ok: true; webhookId: string } | { ok: false; error: string }> {
  const webhookId = headers.get('webhook-id')?.trim() ?? ''
  const webhookTimestamp = headers.get('webhook-timestamp')?.trim() ?? ''
  const webhookSignature = headers.get('webhook-signature')?.trim() ?? ''

  if (!webhookId || !webhookTimestamp || !webhookSignature) {
    return { ok: false, error: 'missing_webhook_headers' }
  }

  if (!/^\d+$/.test(webhookTimestamp)) {
    return { ok: false, error: 'invalid_webhook_timestamp' }
  }

  const ts = Number.parseInt(webhookTimestamp, 10)
  if (!Number.isSafeInteger(ts) || Math.abs(nowSec - ts) > WEBHOOK_TOLERANCE_SECONDS) {
    return { ok: false, error: 'webhook_timestamp_out_of_range' }
  }

  const expected = await computeStandardWebhookSignature(
    secret,
    webhookId,
    webhookTimestamp,
    rawBody,
  )

  const candidates = webhookSignature.split(/\s+/).filter((token) => token.length > 0)
  for (const token of candidates) {
    const commaIdx = token.indexOf(',')
    if (commaIdx === -1) continue
    const version = token.slice(0, commaIdx)
    const sig = token.slice(commaIdx + 1)
    if (version === 'v1' && sig.length > 0 && timingSafeStringEqual(sig, expected)) {
      return { ok: true, webhookId }
    }
  }

  return { ok: false, error: 'invalid_webhook_signature' }
}

function parseIsoTimestampMs(raw: unknown): number | null {
  if (typeof raw !== 'string' || raw.trim().length === 0) return null
  const ms = Date.parse(raw)
  return Number.isFinite(ms) ? ms : null
}

function nonEmptyString(raw: unknown): string | null {
  if (typeof raw !== 'string') return null
  const trimmed = raw.trim()
  return trimmed.length > 0 ? trimmed : null
}

async function resolveWebhookUserId(
  db: D1Database,
  data: PolarSubscriptionPayload,
  subscriptionId: string | null,
  customerId: string | null,
): Promise<string | null> {
  const candidates: string[] = []
  const metaUserId = nonEmptyString(data.metadata?.user_id)
  if (metaUserId) candidates.push(metaUserId)

  const externalCustomerId = nonEmptyString(data.customer?.external_id)
  if (externalCustomerId && !candidates.includes(externalCustomerId)) {
    candidates.push(externalCustomerId)
  }

  for (const candidate of candidates) {
    const user = await db
      .prepare('SELECT id FROM users WHERE id = ?1')
      .bind(candidate)
      .first<{ id: string }>()
    if (user) return user.id
  }

  if (subscriptionId) {
    const bySub = await db
      .prepare(
        `SELECT user_id FROM user_entitlements
         WHERE provider = 'polar' AND provider_subscription_id = ?1`,
      )
      .bind(subscriptionId)
      .first<{ user_id: string }>()
    if (bySub) return bySub.user_id
  }

  if (customerId) {
    const byCust = await db
      .prepare(
        `SELECT user_id FROM user_entitlements
         WHERE provider = 'polar' AND provider_customer_id = ?1`,
      )
      .bind(customerId)
      .first<{ user_id: string }>()
    if (byCust) return byCust.user_id
  }

  return null
}

export async function handleCreatePolarCheckout(
  request: Request,
  env: Env,
  userId: string,
  authMethod: 'jwt' | 'dev_token',
  now = Date.now(),
): Promise<Response> {
  if (authMethod === 'dev_token' || !env.DB) {
    return jsonResponse(
      {
        error: 'billing_unavailable',
        message: 'rem Premium checkout requires a signed-in account.',
      },
      400,
    )
  }

  if (!isPolarConfigured(env)) {
    return jsonResponse(
      {
        error: 'polar_not_configured',
        message: 'Web billing is not configured on this server yet.',
      },
      503,
    )
  }

  const current = await checkSyncEntitlement(env, userId, authMethod, now)
  const ent = current.entitlement

  if (
    ent.provider === 'apple' &&
    (ent.state === 'active' || ent.state === 'past_due')
  ) {
    return jsonResponse(
      {
        error: 'already_subscribed_apple',
        message:
          'Your rem Premium subscription is already active via the Apple App Store. Manage it in iOS Subscriptions.',
      },
      409,
    )
  }

  if (ent.state === 'active') {
    return jsonResponse(
      {
        error: 'already_subscribed',
        message: 'Your rem Premium subscription is already active.',
      },
      409,
    )
  }

  if (ent.state === 'invite' && ent.expiresAt === null) {
    return jsonResponse(
      {
        error: 'already_has_free_access',
        message: 'Your account already has indefinite free rem Premium access.',
      },
      409,
    )
  }

  const user = await env.DB.prepare('SELECT email FROM users WHERE id = ?1')
    .bind(userId)
    .first<{ email: string | null }>()

  const origin = new URL(request.url).origin
  const payload: Record<string, unknown> = {
    products: [env.POLAR_PRODUCT_ID!.trim()],
    external_customer_id: userId,
    metadata: { user_id: userId },
    success_url: `${origin}/premium?billing=success`,
  }
  if (user?.email) {
    payload.customer_email = user.email
  }

  const resp = await fetch(`${getPolarApiBase(env)}/v1/checkouts/`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${env.POLAR_ACCESS_TOKEN!.trim()}`,
      'Content-Type': 'application/json',
      Accept: 'application/json',
    },
    body: JSON.stringify(payload),
  })

  if (!resp.ok) {
    return jsonResponse(
      {
        error: 'polar_checkout_failed',
        message: `Could not start checkout (Polar returned ${resp.status}).`,
      },
      502,
    )
  }

  const data = (await resp.json().catch(() => null)) as { url?: unknown } | null
  const checkoutUrl = nonEmptyString(data?.url)
  if (!checkoutUrl) {
    return jsonResponse(
      {
        error: 'polar_checkout_failed',
        message: 'Polar checkout response was missing a checkout URL.',
      },
      502,
    )
  }

  const result: PolarCheckoutResponse = { checkoutUrl }
  return jsonResponse(result)
}

export async function handleCreatePolarPortal(
  env: Env,
  userId: string,
  authMethod: 'jwt' | 'dev_token',
): Promise<Response> {
  if (authMethod === 'dev_token' || !env.DB) {
    return jsonResponse(
      {
        error: 'billing_unavailable',
        message: 'Billing management requires a signed-in account.',
      },
      400,
    )
  }

  if (!isPolarConfigured(env)) {
    return jsonResponse(
      {
        error: 'polar_not_configured',
        message: 'Web billing is not configured on this server yet.',
      },
      503,
    )
  }

  const row = await env.DB.prepare(
    'SELECT provider, provider_customer_id FROM user_entitlements WHERE user_id = ?1',
  )
    .bind(userId)
    .first<{ provider: string | null; provider_customer_id: string | null }>()

  const customerId =
    row?.provider === 'polar' ? nonEmptyString(row.provider_customer_id) : null
  if (!customerId) {
    return jsonResponse(
      {
        error: 'no_polar_customer',
        message: 'No Polar billing profile was found for this account.',
      },
      404,
    )
  }

  const resp = await fetch(`${getPolarApiBase(env)}/v1/customer-sessions/`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${env.POLAR_ACCESS_TOKEN!.trim()}`,
      'Content-Type': 'application/json',
      Accept: 'application/json',
    },
    body: JSON.stringify({ customer_id: customerId }),
  })

  if (!resp.ok) {
    return jsonResponse(
      {
        error: 'polar_portal_failed',
        message: `Could not open billing portal (Polar returned ${resp.status}).`,
      },
      502,
    )
  }

  const data = (await resp.json().catch(() => null)) as {
    customer_portal_url?: unknown
  } | null
  const portalUrl = nonEmptyString(data?.customer_portal_url)
  if (!portalUrl) {
    return jsonResponse(
      {
        error: 'polar_portal_failed',
        message: 'Polar customer session response was missing a portal URL.',
      },
      502,
    )
  }

  const result: PolarPortalResponse = { portalUrl }
  return jsonResponse(result)
}

export async function handlePolarWebhook(
  request: Request,
  env: Env,
  nowMs = Date.now(),
): Promise<Response> {
  const secret = env.POLAR_WEBHOOK_SECRET?.trim()
  if (!env.DB || !secret) {
    return jsonResponse({ error: 'polar_webhook_not_configured' }, 503)
  }

  const rawBody = await request.text()
  const verified = await verifyPolarWebhookSignature(
    secret,
    request.headers,
    rawBody,
    Math.floor(nowMs / 1000),
  )
  if (!verified.ok) {
    return jsonResponse({ error: verified.error }, 401)
  }

  let envelope: PolarWebhookEnvelope
  try {
    envelope = JSON.parse(rawBody) as PolarWebhookEnvelope
  } catch {
    return jsonResponse({ error: 'invalid_json' }, 400)
  }

  const eventType = nonEmptyString(envelope?.type)
  if (!eventType || !envelope.data || typeof envelope.data !== 'object') {
    return jsonResponse({ error: 'invalid_webhook_payload' }, 400)
  }

  const inserted = await env.DB.prepare(
    `INSERT OR IGNORE INTO billing_events(provider, event_id, processed_at)
     VALUES ('polar', ?1, ?2)`,
  )
    .bind(verified.webhookId, nowMs)
    .run()

  if ((inserted.meta.changes ?? 0) === 0) {
    return jsonResponse({ received: true, duplicate: true })
  }

  if (!POLAR_SUBSCRIPTION_EVENTS.has(eventType)) {
    return jsonResponse({ received: true, ignored: true })
  }

  const data = envelope.data
  const subscriptionId = nonEmptyString(data.id)
  const customerId = nonEmptyString(data.customer_id) ?? nonEmptyString(data.customer?.id)
  const userId = await resolveWebhookUserId(env.DB, data, subscriptionId, customerId)
  if (!userId) {
    return jsonResponse({ received: true, ignored: true, reason: 'user_not_found' })
  }

  const rawStatus = nonEmptyString(data.status)?.toLowerCase() ?? ''
  if (rawStatus === 'incomplete') {
    return jsonResponse({ received: true, ignored: true, reason: 'subscription_incomplete' })
  }

  let periodEnd =
    parseIsoTimestampMs(data.current_period_end) ?? parseIsoTimestampMs(data.ends_at)
  const endedAt = parseIsoTimestampMs(data.ended_at)
  let cancelAtPeriodEnd = Boolean(data.cancel_at_period_end)
  let mappedStatus: 'active' | 'past_due' | 'canceled'

  if (
    eventType === 'subscription.revoked' ||
    rawStatus === 'unpaid' ||
    rawStatus === 'incomplete_expired'
  ) {
    mappedStatus = 'canceled'
    cancelAtPeriodEnd = true
    periodEnd = endedAt ?? (periodEnd !== null ? Math.min(periodEnd, nowMs) : nowMs)
  } else if (rawStatus === 'past_due') {
    mappedStatus = 'past_due'
  } else if (eventType === 'subscription.uncanceled') {
    mappedStatus = 'active'
    cancelAtPeriodEnd = false
  } else if (eventType === 'subscription.canceled' || rawStatus === 'canceled') {
    cancelAtPeriodEnd = true
    const hasRemainingPeriod =
      endedAt === null && periodEnd !== null && periodEnd > nowMs
    mappedStatus = hasRemainingPeriod ? 'active' : 'canceled'
  } else {
    mappedStatus = 'active'
  }

  // Never overwrite an active Apple App Store subscription with a stale Polar cancellation/revocation.
  if (mappedStatus === 'canceled') {
    const existing = await env.DB.prepare(
      'SELECT provider, status FROM user_entitlements WHERE user_id = ?1',
    )
      .bind(userId)
      .first<{ provider: string | null; status: string }>()
    if (existing?.provider === 'apple' && existing.status === 'active') {
      return jsonResponse({ received: true, ignored: true, reason: 'active_apple_subscription' })
    }
  }

  await env.DB.prepare(
    `INSERT INTO user_entitlements (
       user_id, status, plan, provider, provider_customer_id, provider_subscription_id,
       current_period_end, cancel_at_period_end, trial_started_at, original_transaction_id, updated_at
     ) VALUES (?1, ?2, 'premium_yearly', 'polar', ?3, ?4, ?5, ?6, NULL, NULL, ?7)
     ON CONFLICT(user_id) DO UPDATE SET
       status = excluded.status,
       plan = excluded.plan,
       provider = 'polar',
       provider_customer_id = COALESCE(excluded.provider_customer_id, user_entitlements.provider_customer_id),
       provider_subscription_id = COALESCE(excluded.provider_subscription_id, user_entitlements.provider_subscription_id),
       current_period_end = excluded.current_period_end,
       cancel_at_period_end = excluded.cancel_at_period_end,
       updated_at = excluded.updated_at`,
  )
    .bind(
      userId,
      mappedStatus,
      customerId,
      subscriptionId,
      periodEnd,
      cancelAtPeriodEnd ? 1 : 0,
      nowMs,
    )
    .run()

  return jsonResponse({ received: true })
}

export async function revokePolarSubscriptionForAccountDeletion(
  env: Env,
  userId: string,
): Promise<void> {
  if (!env.DB || !env.POLAR_ACCESS_TOKEN?.trim()) return
  let row: {
    provider: string | null
    status: string
    provider_subscription_id: string | null
  } | null = null
  try {
    row = await env.DB.prepare(
      `SELECT provider, status, provider_subscription_id
       FROM user_entitlements WHERE user_id = ?1`,
    )
      .bind(userId)
      .first<{
        provider: string | null
        status: string
        provider_subscription_id: string | null
      }>()
  } catch {
    return
  }

  if (
    row?.provider !== 'polar' ||
    !row.provider_subscription_id ||
    (row.status !== 'active' && row.status !== 'past_due')
  ) {
    return
  }

  try {
    await fetch(
      `${getPolarApiBase(env)}/v1/subscriptions/${encodeURIComponent(row.provider_subscription_id)}`,
      {
        method: 'DELETE',
        headers: {
          Authorization: `Bearer ${env.POLAR_ACCESS_TOKEN.trim()}`,
          Accept: 'application/json',
        },
      },
    )
  } catch {
    // Best-effort cancellation: never block user-requested account deletion.
  }
}
