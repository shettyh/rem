# Cloud Sync (Cloudflare Workers + Durable Objects + R2) and Mobile Companion Design

_Date: 2026-11-21 · Status: approved architectural direction for phased implementation_

## 1. Summary & Goals

Make `rem` available across desktop (macOS, Linux, Windows), CLI, and mobile (iPhone first) with dependable, low-cost, local-first cloud sync—while keeping the entire project **Apache-2.0 open source**, preserving local-only operation, and keeping `rusqlite` + `rem-core` as the single source of truth for storage and FSRS-6 scheduling.

### Key product decisions

- **Local-first by default:** No startup login wall. Creating cards, studying, CLI capture, draft triage, and JSON backup/export continue to work completely offline with no account.
- **Two sync modes only: `Local only` and `Cloud sync`.** Existing Git sync (`GitSyncService`, system `git` shelling, `src-tauri/src/git.rs`) is cumbersome across devices and will be retired cleanly once Cloud sync is ready (JSON export/import remains for portability).
- **No GitHub Auth required:** Use **Sign in with Apple** (native on iOS/macOS, required on iOS alongside social logins) and **Sign in with Google** (cross-platform), with optional email OTP later if needed.
- **Backend stack:** **Cloudflare Workers + SQLite-backed Durable Objects (1 per user) + R2** for media assets + **D1** for auth/account records.
- **Zero halting conflict UI:** Make `rem-core` mutations commutative (per-domain HLCs + append-only review events) so concurrent edits and reviews converge automatically.
- **Shared Rust core everywhere:** Desktop, CLI, and iPhone share `rem-core` (SQLite + FSRS-6 + `StudySession`) and a new `rem-sync` crate (HTTP client + outbox runner + asset sync).

---

## 2. System Architecture

```text
┌──────────────────────────────────────────────────────────┐
│  Desktop App  /  rem CLI  /  iPhone Companion            │
│   ├── rem-core (rusqlite WAL + FSRS-6 + sync_outbox)     │
│   └── rem-sync (Rust HTTPS sync client + asset uploader) │
└────────────────────────────┬─────────────────────────────┘
                             │ HTTPS (short-lived JWT)
                             ▼
┌──────────────────────────────────────────────────────────┐
│  Cloudflare Worker (Stateless API Router + Auth Gate)    │
│   ├── /v1/auth/*      ──► Apple/Google OAuth + D1        │
│   ├── /v1/sync        ──► UserSyncDO (1 SQLite DO/user)  │
│   └── /v1/assets/*    ──► R2 (users/<uid>/assets/<sha>)  │
└──────────────────────────────────────────────────────────┘
```

### Why this topology fits `rem`

1. **Physical tenant isolation:** `env.USER_SYNC.idFromName(verifiedUserId)` routes each user to their own isolated SQLite Durable Object. There are no shared-table RLS policies to misconfigure.
2. **Strict commit ordering for free:** A Durable Object is single-threaded per user. `INTEGER PRIMARY KEY AUTOINCREMENT` gives a strictly ordered, gap-free `seq` cursor per user without locks, ETag retry loops, or Postgres sequence gaps.
3. **Incremental payloads:** Grading a card pushes ~300 bytes instead of uploading a whole-collection snapshot.
4. **Keep `rusqlite` untouched:** Desktop and `rem` CLI continue sharing the same local SQLite WAL database on macOS, Linux, and Windows.
5. **Low predictable cost:** Workers Paid ($5/month base) + Durable Objects SQLite + D1 + R2 ($0 egress for images/GIFs) keeps operating costs around ~$5–$15/month up to ~1,000 active users.
6. **Optional real-time sync:** The same Durable Object supports Hibernatable WebSockets to poke active devices when another device pushes, sleeping at $0 when idle.

---

## 3. Local `rem-core` Data Model (SQLite Schema v3)

All local writers (desktop app, `rem` CLI, draft acceptance, backup import, mobile) pass through `rem-core` write transactions.

### Local-only vs. Synced tables

| Table | Synced? | Notes |
| --- | --- | --- |
| `decks` | Yes | Synced via `DeckUpsert` / `DeckDelete` |
| `cards` | Yes | Split into independent content, suspension, and scheduling domains |
| `review_logs` | Yes | Immutable UUID-keyed FSRS training logs; union on merge |
| `tombstones` | Yes | Durable deletions for decks and cards; delete wins |
| `assets` | Yes (via R2) | Immutable SHA-256 content-addressed images/GIFs |
| `card_drafts` | **No (local-only)** | Preserves privacy of agent provenance/locators |
| `daily_stats` | **No (local-only)** | Per-device daily cap counters |
| `sync_meta` / `sync_outbox` / `synced_assets` | **No (local-only)** | Sync bookkeeping |

### Schema v3 additions

1. **Hybrid Logical Clock (HLC) columns** — lexicographically sortable strings formatted as `<epoch_ms_13digits>-<counter_4digits>-<device_id_8chars>` so ties break deterministically and minor device clock skew is absorbed automatically:
   - `decks`: add `hlc TEXT NOT NULL DEFAULT ''`
   - `cards`: add `content_hlc TEXT NOT NULL DEFAULT ''`, `suspended_hlc TEXT NOT NULL DEFAULT ''`, `scheduling_hlc TEXT NOT NULL DEFAULT ''`
   - `tombstones`: add `hlc TEXT NOT NULL DEFAULT ''`
2. **Sync bookkeeping tables:**
   ```sql
   CREATE TABLE sync_meta (
     key TEXT PRIMARY KEY,
     value TEXT NOT NULL
     -- keys: sync_mode ('local'|'cloud'), server_url, account_id, device_id, last_seq, last_hlc
   );

   CREATE TABLE sync_outbox (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     op_id TEXT UNIQUE NOT NULL,
     payload_json TEXT NOT NULL,
     created_at INTEGER NOT NULL
   );

   CREATE TABLE synced_assets (
     hash TEXT PRIMARY KEY,
     uploaded_at INTEGER NOT NULL
   );
   ```

### Transactional outbox invariant

- When `sync_mode = 'cloud'`, every synced `Collection` write (`create_deck`, `update_deck`, `delete_deck`, `create_cards`, `update_card`, `delete_card`, `commit_review`, `import_decks`) writes its local row update **and** appends the corresponding op to `sync_outbox` in the **same SQLite `BEGIN IMMEDIATE` transaction**.
- When `sync_mode = 'local'`, nothing is enqueued to `sync_outbox`.
- Because `rem` CLI uses `rem-core` directly, CLI mutations while the app is closed—or concurrently during a sync—are atomically captured in `sync_outbox` and never lost.

---

## 4. Operation Vocabulary & Commutative Merge Rules

### The 7 sync operations

1. `DeckUpsert { id, name, color, settings, created_at, hlc }`
2. `DeckDelete { id, hlc }`
3. `CardCreate { id, deck_id, front, back, tags, created_at, initial_due, hlc }`
4. `CardContentPatch { id, front, back, tags, hlc }` — emitted when editing `front`, `back`, or user `tags`.
5. `CardSuspendPatch { id, suspended, hlc }` — emitted when toggling `suspended`.
6. `CardReview { card_id, deck_id, reviewed_at, grade, scheduling, last_again_at, leech_suspended, leech_tag_added, review_log_id, hlc }` — emitted on **every** `commit_review` (including fixed learning/relearning steps where `review_log_id` is `None`).
7. `CardDelete { id, hlc }`

### Merge rules (`apply_remote_ops`)

All remote ops in a batch apply inside one local SQLite `write_transaction`:

- **Tombstones win:** If `id` (or a card's `deck_id`) exists in `tombstones`, ignore incoming create/update/review ops for it. Deletions cannot be resurrected by stale offline devices, and orphaned card ops never trigger SQLite foreign-key errors.
- **Deck upsert:** Apply iff `op.hlc > deck.hlc` (or deck does not exist and is not tombstoned).
- **Card content vs. scheduling independence:**
  - `CardContentPatch` updates `front`, `back`, `tags` iff `op.hlc > card.content_hlc`. Never overwrites `scheduling`, `due`, or `suspended`.
  - `CardSuspendPatch` updates `suspended` iff `op.hlc > card.suspended_hlc`.
  - `CardReview`:
    1. Always `INSERT OR IGNORE` into `review_logs` when `review_log_id` is present (immutable FSRS history is never dropped).
    2. Update `scheduling`, `due`, `last_again_at` (and any `leech_suspended` / `leech_tag_added` effects) iff `(op.scheduling.reps, op.hlc) > (card.scheduling.reps, card.scheduling_hlc)`.
- **Key outcomes:**
  - *Desktop content edit + phone review* → both survive automatically.
  - *Rare concurrent offline reviews of the same card* → all review logs are preserved, and the further-progressed/newer schedule wins deterministically across all devices with zero sync halt or conflict modal.
  - *Idempotent retries* → re-applying any op is a no-op.
- **Outbox cleanup:** Delete only `sync_outbox` rows whose `op_id` is explicitly in the server's `ackedOpIds`. Any concurrent local/CLI writes made mid-sync stay queued.

---

## 5. Cloudflare Backend & Wire Contract

The wire contract is intentionally minimal and backend-agnostic (3 sync/asset endpoints + auth):

### Durable Object (`UserSyncDO`) schema

```sql
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
```

### Sync endpoint: `POST /v1/sync`

Authenticated via `Authorization: Bearer <access_jwt>`. The Worker extracts `userId` strictly from the verified JWT and forwards to `UserSyncDO(userId)`.

- **Request:**
  ```json
  {
    "deviceId": "dev-iphone-1",
    "sinceSeq": 142,
    "limit": 500,
    "ops": [
      { "opId": "uuid-1", "payload": { "type": "cardReview", "...": "..." } }
    ]
  }
  ```
- **Server transaction (`ctx.storage.transactionSync`):**
  1. `INSERT OR IGNORE INTO ops(op_id, device_id, payload, created_at)` for each incoming op (deduplicating retries).
  2. Query `SELECT seq, op_id, device_id, payload FROM ops WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2`.
  3. Query `latestSeq = COALESCE(MAX(seq), 0)`.
- **Response:**
  ```json
  {
    "latestSeq": 145,
    "hasMore": false,
    "ackedOpIds": ["uuid-1"],
    "ops": [
      { "seq": 143, "opId": "uuid-other", "deviceId": "dev-mac-1", "payload": { "...": "..." } }
    ]
  }
  ```

### Asset endpoints (R2: `users/<userId>/assets/<sha256>`)

- `POST /v1/assets/missing` — `{ hashes: [...] }` → checks the user's DO `assets` table (zero R2 reads) and returns `{ missing: [...] }`.
- `PUT /v1/assets/:hash` — verifies SHA-256 of request body matches `:hash`, enforces per-file limit (e.g. 10 MB) and per-user quota in DO, writes object to R2, records metadata in DO.
- `GET /v1/assets/:hash` — streams object from R2 (`Cache-Control: private, max-age=31536000, immutable`).

### Client sync order (`rem-sync`)

1. **Upload missing assets first:** Scan pending `sync_outbox` ops with `asset_refs()`, call `/v1/assets/missing`, upload missing blobs via `PUT /v1/assets/:hash`, mark in `synced_assets`. Never publish a card op whose referenced image is not yet in R2.
2. **Push & pull ops:** Loop `POST /v1/sync` (batches of up to ~200 outbox ops) until outbox is empty and `hasMore == false`.
3. **Commit locally:** In one SQLite transaction, apply remote ops (`deviceId != self`), advance `last_hlc` and `last_seq`, and delete `ackedOpIds` from `sync_outbox`.
4. **Download newly referenced assets:** Fetch any `asset:<hash>` referenced in newly pulled cards not yet present in local `assets`, verify SHA-256, and store locally.

### Bootstrap & Compaction

- **First cloud connect (existing local collection):** Enqueue deterministic bootstrap ops (`op_id = "boot:<device_id>:<entity_id>"`) preserving all existing deck/card/review UUIDs, then run the normal sync loop. Connecting two non-empty collections merges both cleanly without deleting either.
- **Compaction (v1.1):** Once a user's `ops` count exceeds a threshold (e.g. 10,000 ops), write a compacted gzipped snapshot at `snapshot_seq = N` to R2 (`users/<uid>/snapshot.json.gz`) and prune `ops <= N`. Brand-new devices fetch `GET /v1/bootstrap` (snapshot + `ops > N`).
- **Real-time pokes (v1.1):** Optional Hibernatable WebSocket to `UserSyncDO` while foregrounded. On push, DO sends `{ type: "poke", latestSeq }` to other connected devices, which trigger the normal `POST /v1/sync`.

---

## 6. Auth, Security & Account Lifecycle

- **Providers:** **Sign in with Apple** + **Sign in with Google** (no GitHub OAuth; optional email OTP later).
- **OAuth flow:** Native system browser / `ASWebAuthenticationSession` with PKCE → deep-link callback `rem://auth/callback?code=...` → Worker exchanges code, creates/fetches user in D1 (`users`, `auth_identities`), issues short-lived JWT access token (15 min) + rotating opaque refresh token (hashed in D1 `refresh_tokens`).
- **Credential storage:** Tokens live exclusively in OS Keychain (`keyring` on desktop / iOS Keychain on mobile) and native memory—never in SQLite, backups, logs, or webview `localStorage`.
- **Account binding:** `sync_meta` binds the local database to `(server_url, account_id)`. Signing out clears credentials and pauses sync without deleting local cards. Signing into a different account on an already-bound DB is rejected unless the user explicitly resets or switches collections.
- **Account deletion:** `DELETE /v1/account` deletes the user's R2 prefix, calls `ctx.storage.deleteAll()` on `UserSyncDO`, and removes user/token rows in D1.

---

## 7. Open-Source & Self-Hosting Model

All code—client, Rust core, and sync server—remains licensed under **Apache-2.0**:

1. **Everything lives in the public repo:** `rem-core`, `rem-sync`, desktop/mobile apps, and `sync-server/` (Cloudflare Worker, `UserSyncDO`, D1 migrations, R2 setup, `wrangler.toml`, protocol tests). No closed-source "enterprise" sync logic.
2. **Configurable Server URL in the standard binary:** Users can either use an optional hosted `rem` Cloud instance or point the ordinary released app at their own server URL without rebuilding or needing license keys.
3. **Two self-hosting paths:**
   - **BYO Cloudflare (Day 1):** Clone the repo and deploy `sync-server/` to your own Cloudflare account (`wrangler deploy`), then set the Server URL in `rem` Settings.
   - **Generic VPS / Docker (optional later):** Because `rem-sync` only talks to 3 standard HTTP endpoints (no Cloudflare SDK on the client), a ~300-line Rust/Axum + SQLite + local-disk server can implement the exact same wire contract for fully non-Cloudflare self-hosting.
4. **Sustainable optional hosting:** If you offer a hosted `rem` Cloud later, users pay for hosting/convenience (covering Cloudflare + Apple Developer fees), never for locked features.

---

## 8. Phased Implementation Plan

| Slice | Scope | Acceptance Gate |
| --- | --- | --- |
| **1. `rem-core` sync engine** | Schema v3 migration, HLC helper, 7 op types, transactional `sync_outbox`, `apply_remote_ops`, bootstrap generator | Pure Rust test suite with two temp SQLite DBs: offline edit + review, dual offline review, delete + stale edit, CLI write mid-sync, duplicate retry dedupe, non-empty collection merge. |
| **2. Cloudflare `sync-server` + `rem-sync`** | Worker, `UserSyncDO`, R2 asset routes, dev bearer token, Rust `rem-sync` crate | `vitest-pool-workers` tests + live Cloudflare deploy + two desktop collections syncing cards, reviews, deletions, and images end-to-end. |
| **3. Auth, Desktop UI & Git sync retirement** | Apple + Google OAuth in Worker + D1, OS Keychain integration, Settings (`Local only` / `Cloud sync`, configurable Server URL), account deletion, remove legacy Git sync | Two-user isolation test, token refresh, sign-out/re-login, account-switch guard, clean removal of Git sync with full CI green. |
| **4. iPhone Companion** | Focused mobile review & quick-capture app sharing `rem-core` + `rem-sync` (SwiftUI via UniFFI *or* Tauri Mobile—decided before Slice 4) | Physical iPhone offline grading, force-quit/reopen, airplane-mode reconnect, media offline readiness, desktop ↔ iPhone convergence. |
