# `rem-sync-server`

Open-source (Apache-2.0) Cloudflare Workers + SQLite Durable Objects + D1 + R2 sync backend for [`rem`](../README.md).

Each authenticated user is physically isolated in their own SQLite-backed Durable Object (`UserSyncDO`) with media assets stored in R2 under `users/<userId>/assets/<sha256>` and OAuth/session/entitlement records in D1.

## Endpoints

| Route | Purpose |
| --- | --- |
| `GET /health` | Liveness check |
| `GET /premium`, `GET /cloud` | rem Premium overview & logged-in web portal (trial activation + invite-code redemption) |
| `GET /privacy`, `GET /terms`, `GET /support` | Privacy Policy, Terms of Use & Support pages (App Store Connect & iOS subscription sheet) |
| `GET /v1/auth/providers` | Reports enabled sign-in providers (`google`, `apple`, `devToken`) |
| `GET /v1/auth/:provider/start` | Starts Google or Apple OAuth + PKCE login (`127.0.0.1` loopback, `rem://auth/callback`, or same-origin `/premium` & `/cloud`) |
| `GET\|POST /v1/auth/:provider/callback` | Verifies OIDC ID token (JWKS), upserts D1 account, issues one-time PKCE code |
| `POST /v1/auth/exchange` | Exchanges one-time code + PKCE `codeVerifier` for 15-min JWT + rotating refresh token |
| `POST /v1/auth/refresh` | Rotates refresh token (with family reuse detection) and issues a fresh access JWT |
| `POST /v1/auth/logout` | Revokes refresh token family |
| `GET /v1/account` | Returns authenticated account profile (`id`, `email`, `providers`, `authMethod`, structured `entitlement`, and legacy fields) |
| `DELETE /v1/account` | Permanently deletes user R2 assets, `UserSyncDO` storage, and D1 auth/entitlement records |
| `POST /v1/trial/start` | Opt-in start of the one-time no-card free trial (`TRIAL_DAYS`, default `30`) |
| `POST /v1/invites/redeem` | Atomically redeems an invite code (`REM-XXXX-XXXX`) and unlocks sync |
| `POST /v1/billing/polar/checkout` | Creates a Polar Checkout Session for rem Premium (`$5/year`) and returns `{ checkoutUrl }` |
| `POST /v1/billing/polar/portal` | Creates a Polar Customer Portal session for existing Polar subscribers and returns `{ portalUrl }` |
| `POST /v1/webhooks/polar` | Receives Polar subscription webhooks (Standard Webhooks HMAC-SHA256 verified + idempotent via `billing_events`) |
| `POST /v1/billing/apple/verify` | Verifies StoreKit 2 `signedTransactionInfo` JWS against Apple Root CA - G3 and activates `provider='apple'` entitlement |
| `POST /v1/webhooks/apple` | Receives App Store Server Notifications v2 (`signedPayload` JWS verified + idempotent via `billing_events`) |
| `POST /v1/sync` | Push local outbox ops and pull remote ops after `sinceSeq` |
| `POST /v1/assets/missing` | Check which SHA-256 asset hashes are not yet stored |
| `PUT /v1/assets/:hash` | Upload content-addressed media asset (SHA-256 verified + quota checked) |
| `GET /v1/assets/:hash` | Stream immutable private media asset from R2 |

## Local development & tests

```bash
cd sync-server
npm install
npm test
```

To run locally with a dev bearer token (no OAuth setup required):

```bash
cat <<'EOF' > .dev.vars
DEV_AUTH_TOKEN=local-dev-secret
EOF
npm run dev
```

Send `Authorization: Bearer local-dev-secret` (default user `dev-user`) or `Authorization: Bearer local-dev-secret:<userId>` to isolate multiple test users locally.

## BYO Cloudflare deployment

```bash
cd sync-server
npm install
npx wrangler r2 bucket create rem-sync-assets
npx wrangler d1 create rem-sync-auth
# Update database_id in wrangler.toml, then apply migrations:
npx wrangler d1 migrations apply rem-sync-auth --remote
npx wrangler secret put JWT_SECRET
# Optional: self-hosted personal token auth
npx wrangler secret put DEV_AUTH_TOKEN
# Optional: Google OAuth (register https://<worker>/v1/auth/google/callback)
npx wrangler secret put GOOGLE_CLIENT_ID
npx wrangler secret put GOOGLE_CLIENT_SECRET
# Optional: Sign in with Apple (register https://<worker>/v1/auth/apple/callback)
npx wrangler secret put APPLE_CLIENT_ID
npx wrangler secret put APPLE_TEAM_ID
npx wrangler secret put APPLE_KEY_ID
npx wrangler secret put APPLE_PRIVATE_KEY
# Optional: gate sync/R2 access behind rem Premium / trial / invites (default: open for self-hosting)
npx wrangler secret put REQUIRE_SUBSCRIPTION # set to "true" to enforce entitlements (HTTP 402 otherwise)
npx wrangler secret put ALLOWED_EMAILS       # optional comma-separated owner/VIP emails always allowed to sync
npx wrangler secret put UPGRADE_URL          # optional pricing URL (e.g. https://rem-sync.shettyh.fyi/premium)
# Optional: Polar Merchant-of-Record billing for Desktop & Web ($5/year)
npx wrangler secret put POLAR_ACCESS_TOKEN   # Polar Organization Access Token (checkouts:write, customer_sessions:write, subscriptions:write)
npx wrangler secret put POLAR_PRODUCT_ID     # Polar annual rem Premium product UUID
npx wrangler secret put POLAR_WEBHOOK_SECRET # Polar Standard Webhooks endpoint secret
# Optional: set POLAR_API_BASE=https://sandbox-api.polar.sh for Polar Sandbox testing
# Optional: Apple StoreKit 2 IAP & App Store Server Notifications v2 ($4.99/year on iOS)
# Set in wrangler.toml [vars] or as secrets:
#   APPLE_BUNDLE_ID=com.shettyh.rem
#   APPLE_PRODUCT_ID=com.shettyh.rem.premium.yearly
#   APPLE_ENVIRONMENT=Production,Sandbox   # or "Production" / "Sandbox" (Production,Sandbox allows TestFlight & App Review)
npm run deploy
```

## Managing invite codes in D1

Use the built-in CLI (`sync-server/scripts/invites.mjs`) to generate, list, and revoke Crockford-style invite codes (`REM-XXXX-XXXX`):

```bash
cd sync-server
# Create a single-use 1-year invite code on production D1
npm run invites -- create --uses 1 --days 365 --note "beta tester" --remote

# Create an indefinite free access invite code
npm run invites -- create --uses 1 --forever --note "contributor" --remote

# List recent invite codes and redemption counts
npm run invites -- list --remote

# Revoke a code
npm run invites -- revoke REM-ABCD-EFGH --remote
```

## Security, scheduled maintenance & CI deploys

- **Invite redemption rate limiting:** `POST /v1/invites/redeem` enforces a D1-backed fixed-window throttle (10 attempts / 15 min per user and 20 attempts / 15 min per client IP via `CF-Connecting-IP` / `X-Forwarded-For`), returning `HTTP 429` with `Retry-After`.
- **Daily auth/table cleanup:** `wrangler.toml` registers a daily cron trigger (`0 3 * * *`) invoking `scheduled()`, which purges expired `oauth_codes`, expired refresh tokens, refresh tokens revoked more than 7 days ago (preserving recent revocations for token-family reuse detection), and stale `rate_limits` buckets.
- **Automated deploys (`.github/workflows/sync-server-deploy.yml`):** Pushes to `main` touching `sync-server/` (or manual `workflow_dispatch`) run typecheck + `vitest-pool-workers`, apply remote D1 migrations, and deploy the Worker when `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, and `CLOUDFLARE_D1_DATABASE_ID` secrets are configured (and optionally smoke-check `vars.SYNC_SERVER_HEALTH_URL`). Forks without those secrets skip deploy cleanly.
