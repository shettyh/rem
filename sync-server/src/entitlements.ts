import type {
  AccountInfo,
  EntitlementInfo,
  EntitlementState,
  Env,
  SubscriptionStatus,
} from './types'

const DEFAULT_TRIAL_DAYS = 30
const DEFAULT_PRICE_DISPLAY = '$5/year'
const DAY_MS = 86_400_000

export interface SyncEntitlementResult {
  syncEnabled: boolean
  subscriptionRequired: boolean
  subscriptionStatus: SubscriptionStatus
  upgradeUrl: string | null
  entitlement: EntitlementInfo
}

interface EntitlementRow {
  status: string
  plan: string | null
  provider: string | null
  provider_customer_id: string | null
  provider_subscription_id: string | null
  current_period_end: number | null
  cancel_at_period_end: number | null
  trial_started_at: number | null
  original_transaction_id: string | null
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

export function isSubscriptionRequired(env: Env): boolean {
  return env.REQUIRE_SUBSCRIPTION?.trim().toLowerCase() === 'true'
}

export function getUpgradeUrl(env: Env): string | null {
  const trimmed = env.UPGRADE_URL?.trim()
  return trimmed && trimmed.length > 0 ? trimmed : null
}

export function getTrialDays(env: Env): number {
  const raw = env.TRIAL_DAYS?.trim()
  if (!raw) return DEFAULT_TRIAL_DAYS
  const parsed = Number.parseInt(raw, 10)
  return Number.isFinite(parsed) && parsed > 0 && parsed <= 365 ? parsed : DEFAULT_TRIAL_DAYS
}

export function getPriceDisplay(env: Env): string {
  const trimmed = env.CLOUD_PRICE_DISPLAY?.trim()
  return trimmed && trimmed.length > 0 ? trimmed : DEFAULT_PRICE_DISPLAY
}

export function isPolarConfigured(env: Env): boolean {
  return Boolean(env.POLAR_ACCESS_TOKEN?.trim() && env.POLAR_PRODUCT_ID?.trim())
}

function isEmailAllowlisted(env: Env, email: string | null): boolean {
  if (!email || !env.ALLOWED_EMAILS) return false
  const target = email.trim().toLowerCase()
  if (!target) return false
  return env.ALLOWED_EMAILS.split(',')
    .map((entry) => entry.trim().toLowerCase())
    .filter((entry) => entry.length > 0)
    .includes(target)
}

function parseSubscriptionStatus(raw: string | null | undefined): SubscriptionStatus {
  switch (raw) {
    case 'active':
    case 'trialing':
    case 'past_due':
    case 'canceled':
    case 'allowlisted':
      return raw
    default:
      return 'none'
  }
}

async function fetchEntitlementRow(db: D1Database, userId: string): Promise<EntitlementRow | null> {
  try {
    return await db
      .prepare(
        `SELECT status, plan, provider, provider_customer_id, provider_subscription_id,
                current_period_end, cancel_at_period_end, trial_started_at, original_transaction_id
         FROM user_entitlements WHERE user_id = ?1`,
      )
      .bind(userId)
      .first<EntitlementRow>()
  } catch {
    // Fallback if migration 0004 columns have not been applied yet.
    try {
      const legacy = await db
        .prepare(
          `SELECT status, plan, provider, provider_customer_id, provider_subscription_id, current_period_end
           FROM user_entitlements WHERE user_id = ?1`,
        )
        .bind(userId)
        .first<{
          status: string
          plan: string | null
          provider: string | null
          provider_customer_id: string | null
          provider_subscription_id: string | null
          current_period_end: number | null
        }>()
      if (!legacy) return null
      return {
        ...legacy,
        cancel_at_period_end: 0,
        trial_started_at: null,
        original_transaction_id: null,
      }
    } catch {
      return null
    }
  }
}

function makeEntitlementInfo(
  env: Env,
  fields: {
    state: EntitlementState
    syncEnabled: boolean
    provider: string | null
    plan: string | null
    expiresAt: number | null
    cancelAtPeriodEnd: boolean
    canStartTrial: boolean
    canManageBilling?: boolean
  },
): EntitlementInfo {
  const polarConfigured = isPolarConfigured(env)
  const canSubscribePolar =
    polarConfigured &&
    fields.state !== 'self_hosted' &&
    fields.state !== 'active' &&
    !(fields.state === 'past_due' && fields.provider === 'apple')

  return {
    state: fields.state,
    syncEnabled: fields.syncEnabled,
    provider: fields.provider,
    plan: fields.plan,
    expiresAt: fields.expiresAt,
    cancelAtPeriodEnd: fields.cancelAtPeriodEnd,
    canStartTrial: fields.canStartTrial,
    trialDays: getTrialDays(env),
    priceDisplay: getPriceDisplay(env),
    canSubscribePolar,
    canManageBilling: Boolean(fields.canManageBilling && polarConfigured),
  }
}

export async function checkSyncEntitlement(
  env: Env,
  userId: string,
  authMethod: 'jwt' | 'dev_token',
  now = Date.now(),
): Promise<SyncEntitlementResult> {
  const required = isSubscriptionRequired(env)
  const upgradeUrl = getUpgradeUrl(env)

  // 1. Self-hosted mode or dev_token
  if (!required || authMethod === 'dev_token') {
    const entitlement = makeEntitlementInfo(env, {
      state: 'self_hosted',
      syncEnabled: true,
      provider: authMethod === 'dev_token' ? 'dev_token' : 'self_hosted',
      plan: 'self_hosted',
      expiresAt: null,
      cancelAtPeriodEnd: false,
      canStartTrial: false,
    })
    return {
      syncEnabled: true,
      subscriptionRequired: required,
      subscriptionStatus: required ? 'allowlisted' : 'none',
      upgradeUrl,
      entitlement,
    }
  }

  if (!env.DB) {
    const entitlement = makeEntitlementInfo(env, {
      state: 'none',
      syncEnabled: false,
      provider: null,
      plan: null,
      expiresAt: null,
      cancelAtPeriodEnd: false,
      canStartTrial: false,
    })
    return {
      syncEnabled: false,
      subscriptionRequired: true,
      subscriptionStatus: 'none',
      upgradeUrl,
      entitlement,
    }
  }

  // 2. Allowlist (ALLOWED_EMAILS or legacy approved early_access row)
  const user = await env.DB.prepare('SELECT email FROM users WHERE id = ?1')
    .bind(userId)
    .first<{ email: string | null }>()
  const normalizedEmail = user?.email?.trim().toLowerCase() ?? null

  if (isEmailAllowlisted(env, normalizedEmail)) {
    const entitlement = makeEntitlementInfo(env, {
      state: 'active',
      syncEnabled: true,
      provider: 'allowlist',
      plan: 'allowlist',
      expiresAt: null,
      cancelAtPeriodEnd: false,
      canStartTrial: false,
    })
    return {
      syncEnabled: true,
      subscriptionRequired: true,
      subscriptionStatus: 'allowlisted',
      upgradeUrl,
      entitlement,
    }
  }

  if (normalizedEmail) {
    try {
      const earlyAccess = await env.DB.prepare(
        'SELECT status FROM early_access WHERE email = ?1',
      )
        .bind(normalizedEmail)
        .first<{ status: string }>()
      if (earlyAccess?.status === 'approved') {
        const entitlement = makeEntitlementInfo(env, {
          state: 'active',
          syncEnabled: true,
          provider: 'allowlist',
          plan: 'early_access',
          expiresAt: null,
          cancelAtPeriodEnd: false,
          canStartTrial: false,
        })
        return {
          syncEnabled: true,
          subscriptionRequired: true,
          subscriptionStatus: 'allowlisted',
          upgradeUrl,
          entitlement,
        }
      }
    } catch {
      // Ignore if the early_access migration has not been applied yet.
    }
  }

  // 3. user_entitlements row
  const row = await fetchEntitlementRow(env.DB, userId)
  if (!row) {
    const entitlement = makeEntitlementInfo(env, {
      state: 'none',
      syncEnabled: false,
      provider: null,
      plan: null,
      expiresAt: null,
      cancelAtPeriodEnd: false,
      canStartTrial: true,
    })
    return {
      syncEnabled: false,
      subscriptionRequired: true,
      subscriptionStatus: 'none',
      upgradeUrl,
      entitlement,
    }
  }

  const provider = row.provider ?? null
  const plan = row.plan ?? null
  const expiresAt = row.current_period_end ?? null
  const cancelAtPeriodEnd = Boolean(row.cancel_at_period_end)
  const hasPolarCustomer = provider === 'polar' && Boolean(row.provider_customer_id)
  const periodValid = expiresAt === null || expiresAt > now
  const isPaidProvider = provider === 'apple' || provider === 'polar'

  // 3a. Paid active subscription
  if (isPaidProvider && row.status === 'active' && periodValid) {
    const entitlement = makeEntitlementInfo(env, {
      state: 'active',
      syncEnabled: true,
      provider,
      plan,
      expiresAt,
      cancelAtPeriodEnd,
      canStartTrial: false,
      canManageBilling: hasPolarCustomer,
    })
    return {
      syncEnabled: true,
      subscriptionRequired: true,
      subscriptionStatus: 'active',
      upgradeUrl,
      entitlement,
    }
  }

  // 3b. Paid past_due (grace period while now < expiresAt)
  if (isPaidProvider && row.status === 'past_due') {
    const graceActive = expiresAt !== null && expiresAt > now
    const entitlement = makeEntitlementInfo(env, {
      state: 'past_due',
      syncEnabled: graceActive,
      provider,
      plan,
      expiresAt,
      cancelAtPeriodEnd,
      canStartTrial: false,
      canManageBilling: hasPolarCustomer,
    })
    return {
      syncEnabled: graceActive,
      subscriptionRequired: true,
      subscriptionStatus: 'past_due',
      upgradeUrl,
      entitlement,
    }
  }

  // 3c. Manual / complimentary active grant
  if (!isPaidProvider && provider !== 'trial' && row.status === 'active' && periodValid) {
    const entitlement = makeEntitlementInfo(env, {
      state: 'active',
      syncEnabled: true,
      provider: provider ?? 'allowlist',
      plan: plan ?? 'allowlist',
      expiresAt,
      cancelAtPeriodEnd: false,
      canStartTrial: false,
    })
    return {
      syncEnabled: true,
      subscriptionRequired: true,
      subscriptionStatus: 'active',
      upgradeUrl,
      entitlement,
    }
  }

  // 3d. Trialing
  if ((provider === 'trial' || row.status === 'trialing') && row.status === 'trialing' && expiresAt !== null && expiresAt > now) {
    const entitlement = makeEntitlementInfo(env, {
      state: 'trialing',
      syncEnabled: true,
      provider: provider ?? 'trial',
      plan: plan ?? `trial_${getTrialDays(env)}d`,
      expiresAt,
      cancelAtPeriodEnd: false,
      canStartTrial: false,
    })
    return {
      syncEnabled: true,
      subscriptionRequired: true,
      subscriptionStatus: 'trialing',
      upgradeUrl,
      entitlement,
    }
  }

  // 3e. Expired (prior entitlement/trial row exists, no longer active)
  const entitlement = makeEntitlementInfo(env, {
    state: 'expired',
    syncEnabled: false,
    provider,
    plan,
    expiresAt,
    cancelAtPeriodEnd,
    canStartTrial: row.trial_started_at === null,
    canManageBilling: hasPolarCustomer,
  })
  return {
    syncEnabled: false,
    subscriptionRequired: true,
    subscriptionStatus: parseSubscriptionStatus(row.status),
    upgradeUrl,
    entitlement,
  }
}

export async function getAccountInfo(
  env: Env,
  userId: string,
  authMethod: 'jwt' | 'dev_token',
  now = Date.now(),
): Promise<AccountInfo> {
  const result = await checkSyncEntitlement(env, userId, authMethod, now)
  if (authMethod === 'dev_token' || !env.DB) {
    return {
      id: userId,
      email: null,
      providers: authMethod === 'dev_token' ? ['dev_token'] : [],
      authMethod,
      ...result,
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
    ...result,
  }
}

export async function handleStartTrial(
  env: Env,
  userId: string,
  authMethod: 'jwt' | 'dev_token',
  now = Date.now(),
): Promise<Response> {
  if (authMethod === 'dev_token' || !env.DB) {
    const info = await getAccountInfo(env, userId, authMethod, now)
    return jsonResponse(info)
  }

  const userExists = await env.DB.prepare('SELECT id FROM users WHERE id = ?1')
    .bind(userId)
    .first<{ id: string }>()
  if (!userExists) {
    return jsonResponse({ error: 'account_not_found' }, 401)
  }

  const current = await getAccountInfo(env, userId, authMethod, now)
  const state = current.entitlement.state
  if (state === 'self_hosted' || state === 'active' || state === 'trialing') {
    return jsonResponse(current)
  }

  const existing = await fetchEntitlementRow(env.DB, userId)
  if (existing && existing.trial_started_at !== null) {
    return jsonResponse(
      {
        error: 'trial_already_used',
        message: 'The free trial has already been used for this account.',
      },
      409,
    )
  }

  const trialDays = getTrialDays(env)
  const periodEnd = now + trialDays * DAY_MS
  const plan = `trial_${trialDays}d`

  const res = await env.DB.prepare(
    `INSERT INTO user_entitlements (
       user_id, status, plan, provider, provider_customer_id, provider_subscription_id,
       current_period_end, cancel_at_period_end, trial_started_at, original_transaction_id, updated_at
     ) VALUES (?1, 'trialing', ?2, 'trial', NULL, NULL, ?3, 0, ?4, NULL, ?4)
     ON CONFLICT(user_id) DO UPDATE SET
       status = 'trialing',
       plan = excluded.plan,
       provider = 'trial',
       current_period_end = excluded.current_period_end,
       cancel_at_period_end = 0,
       trial_started_at = COALESCE(user_entitlements.trial_started_at, excluded.trial_started_at),
       updated_at = excluded.updated_at
     WHERE user_entitlements.trial_started_at IS NULL`,
  )
    .bind(userId, plan, periodEnd, now)
    .run()

  if ((res.meta.changes ?? 0) === 0) {
    return jsonResponse(
      {
        error: 'trial_already_used',
        message: 'The free trial has already been used for this account.',
      },
      409,
    )
  }

  const updated = await getAccountInfo(env, userId, authMethod, now)
  return jsonResponse(updated)
}
