# `rem-sync-server`

Open-source (Apache-2.0) Cloudflare Workers + SQLite Durable Objects + D1 + R2 sync backend for [`rem`](../README.md).

Each authenticated user is physically isolated in their own SQLite-backed Durable Object (`UserSyncDO`) with media assets stored in R2 under `users/<userId>/assets/<sha256>` and OAuth/session records in D1.

## Endpoints

| Route | Purpose |
| --- | --- |
| `GET /health` | Liveness check |
| `GET /cloud` | Early-access waitlist & self-hosting info page (same-origin with `POST /v1/waitlist`) |
| `GET /v1/auth/providers` | Reports enabled sign-in providers (`google`, `apple`, `devToken`) |
| `GET /v1/auth/:provider/start` | Starts Google or Apple OAuth + PKCE login (`127.0.0.1` loopback or `rem://auth/callback`) |
| `GET\|POST /v1/auth/:provider/callback` | Verifies OIDC ID token (JWKS), upserts D1 account, issues one-time PKCE code |
| `POST /v1/auth/exchange` | Exchanges one-time code + PKCE `codeVerifier` for 15-min JWT + rotating refresh token |
| `POST /v1/auth/refresh` | Rotates refresh token (with family reuse detection) and issues a fresh access JWT |
| `POST /v1/auth/logout` | Revokes refresh token family |
| `POST /v1/waitlist` | Records an early-access waitlist email in D1 (`early_access` table) |
| `GET /v1/account` | Returns authenticated account profile (`id`, `email`, `providers`, `authMethod`, entitlement fields) |
| `DELETE /v1/account` | Permanently deletes user R2 assets, `UserSyncDO` storage, and D1 auth records |
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
# Optional: gate sync/R2 access behind subscriptions or early access (default: open for self-hosting)
npx wrangler secret put REQUIRE_SUBSCRIPTION # set to "true" to enforce entitlements (HTTP 402 otherwise)
npx wrangler secret put ALLOWED_EMAILS       # optional comma-separated owner/VIP emails always allowed to sync
npx wrangler secret put UPGRADE_URL          # optional waitlist/pricing URL shown when HTTP 402 is returned (e.g. https://rem-sync.shettyh.fyi/cloud)
npm run deploy
```

## Managing early-access waitlist in D1

When `REQUIRE_SUBSCRIPTION=true`, users who sign up via `POST /v1/waitlist` land in D1 with `status = 'waitlist'` and remain gated until you approve them (no secret redeploys needed):

```bash
# List pending early-access requests
npx wrangler d1 execute rem-sync-auth --remote \
  --command "SELECT email, note, datetime(created_at/1000, 'unixepoch') AS requested_at FROM early_access WHERE status = 'waitlist' ORDER BY created_at ASC"

# Approve an email (works whether or not they have signed in yet)
npx wrangler d1 execute rem-sync-auth --remote \
  --command "INSERT INTO early_access(email, status, created_at, updated_at) VALUES ('user@example.com', 'approved', unixepoch()*1000, unixepoch()*1000) ON CONFLICT(email) DO UPDATE SET status = 'approved', updated_at = unixepoch()*1000"

# Revoke access for an email
npx wrangler d1 execute rem-sync-auth --remote \
  --command "UPDATE early_access SET status = 'revoked', updated_at = unixepoch()*1000 WHERE email = 'user@example.com'"
```
