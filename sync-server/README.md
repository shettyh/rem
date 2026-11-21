# `rem-sync-server`

Open-source (Apache-2.0) Cloudflare Workers + SQLite Durable Objects + R2 sync backend for [`rem`](../README.md).

Each authenticated user is physically isolated in their own SQLite-backed Durable Object (`UserSyncDO`) with media assets stored in R2 under `users/<userId>/assets/<sha256>`.

## Endpoints

| Route | Purpose |
| --- | --- |
| `GET /health` | Liveness check |
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

To run locally with a dev bearer token:

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
npx wrangler secret put DEV_AUTH_TOKEN
npm run deploy
```
