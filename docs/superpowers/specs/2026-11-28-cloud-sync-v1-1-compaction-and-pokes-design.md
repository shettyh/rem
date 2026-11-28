# Cloud Sync v1.1 — Compaction, Bootstrap Snapshots, and WebSocket Pokes

_Date: 2026-11-28 · Status: active implementation specification_

## 1. Summary & Goals

Cloud Sync v1 shipped an append-only op log in per-user SQLite Durable Objects (`UserSyncDO`) with R2 content-addressed assets and polling/focus-triggered sync. That architecture works cleanly for early collections, but two scaling limits appear over time:

1. **Unbounded op history & slow new-device onboarding:** `UserSyncDO.ops` grows without bound, forcing brand-new devices (or long-offline devices) to replay every historical review and edit from `seq = 0` in 500-op batches.
2. **Polling latency & wire overhead:** Devices rely on 5-minute periodic polling and window focus to discover remote changes, echo their own pushed ops back on every sync, send uncompressed JSON bodies, and transfer assets sequentially.

Cloud Sync v1.1 closes these gaps across five cohesive slices while preserving all v1 invariants: local-first SQLite ownership, commutative HLC/CRDT convergence, zero halting conflict dialogs, open-source self-hostability, and zero Rust dependency in `sync-server` CI/deploy jobs.

### Non-negotiable invariants

- **Single source of merge truth:** Server-side snapshot compaction runs the exact same Rust CRDT/HLC reducer logic as `rem-core` (via a shared pure-Rust crate compiled to WASM), preventing subtle semantic drift between direct op replay and snapshot bootstrap.
- **Non-destructive bootstrap:** Merging a compacted snapshot never wipes local state or drops offline edits/reviews; clients always push/ack local outbox ops before applying a bootstrap snapshot via CRDT merge rules.
- **Safe op pruning:** Compaction writes immutable gzipped snapshots to R2 first; op-log pruning is gated behind an explicit retention window (`RETAIN_TAIL_OPS` and `RETAIN_TAIL_DAYS`) and the `ENABLE_OP_PRUNING` feature flag (`"true"` by default in `sync-server/wrangler.toml`; set `"false"` to keep full op history).
- **Zero background mobile drain:** iOS WebSocket pokes connect strictly while `scenePhase == .active` and disconnect immediately on `.inactive`/`.background`. Auth tokens travel only in HTTP headers, never URL query strings.

---

## 2. Slice 1 — Wire & Client Quick Wins

Low-risk efficiency improvements across `rem-core`, `rem-sync`, and `sync-server`:

1. **Echo suppression (`UserSyncDO.sync`):**
   - Query remote ops with `WHERE seq > ? AND device_id != ? ORDER BY seq ASC LIMIT ?` while still returning `latestSeq` across all ops (including the caller's newly inserted ops).
   - Advance `hasMore` accurately using `limit + 1` lookahead on the filtered query so remote ops are never skipped and cursors never stall when trailing ops belong to the caller.
2. **HTTP gzip compression:**
   - `rem-sync` enables `reqwest` `gzip` response decoding and gzips `POST /v1/sync` request bodies larger than 2 KB (`flate2`, `Content-Encoding: gzip`).
   - `sync-server` transparently accepts both raw JSON and `Content-Encoding: gzip` on `POST /v1/sync` (using `DecompressionStream('gzip')` when not already auto-decompressed by the runtime) with a strict decompressed byte cap (8 MB, returning `413 Payload Too Large` on overflow) to prevent zip bombs.
3. **Push-time outbox coalescing (`rem-core` / `rem-sync`):**
   - When assembling an outbound push batch from `sync_outbox`, fold superseded ops for the same entity: `DeckUpsert`, `CardContentPatch`, `CardSuspendPatch`, and learning-step `CardReview` ops (`review_log_id == None`).
   - Keep all folded local `op_id`s mapped to the surviving representative op so server ACK deletes every folded outbox row atomically, without mutating `sync_outbox` before push (preserving `StudySession::undo_last_grade()` outbox retraction).
4. **Bounded concurrent asset transfers (`rem-sync`):**
   - Upload missing assets and download newly referenced assets with 4-way bounded concurrency (`std::thread::scope` sharing `HttpTransport`/`reqwest::blocking::Client`, keeping `SyncTransport: Send + Sync`).

---

## 3. Slice 2 — Shared Pure-Rust Reducer (`rem-sync-reduce`) & WASM

Extract pure, zero-SQLite, zero-FSRS sync domain primitives and state reduction into a new workspace crate `src-tauri/crates/rem-sync-reduce` depended on by `rem-core`:

- **Shared types & helpers:** `Hlc`, `normalize_card_tags` / `normalize_user_tags` (including `leech` tag handling), `extract_asset_refs` / `asset_refs()`, `SyncOp`, and supporting deck/card/scheduling structs.
- **Versioned `CompactedSnapshot` v1 JSON schema:**
  ```json
  {
    "schemaVersion": 1,
    "snapshotSeq": 12000,
    "maxHlc": "1764320000000-0001-dev12345",
    "decks": [ ... ],
    "cards": [
      {
        "id": "...",
        "deckId": "...",
        "front": "...",
        "back": "...",
        "tags": [],
        "suspended": false,
        "lastAgainAt": null,
        "createdAt": 1764000000000,
        "updatedAt": 1764320000000,
        "scheduling": { ... },
        "contentHlc": "...",
        "suspendedHlc": "...",
        "schedulingHlc": "...",
        "leechHlc": "...",
        "rationale": null,
        "sources": [],
        "proposedBy": null,
        "siblingGroupId": null,
        "clozeIndex": null
      }
    ],
    "reviewLogs": [
      ["log-id", "deck-id", "card-id", 1764320000000, "good"]
    ],
    "tombstones": [{ "id": "...", "kind": "card", "deletedAt": 1764320000000, "hlc": "..." }],
    "reviewLogTombstones": [{ "id": "...", "deletedAt": 1764320000000 }]
  }
  ```
  `reviewLogs` uses compact 5-tuples `[id, deckId, cardId, reviewedAt, grade]` (where `grade` is `"again" | "hard" | "good" | "easy"`) and `reviewLogTombstones` is omitted when empty to minimize snapshot JSON size and memory pressure inside the Worker isolate.
- **Incremental streaming `SnapshotReducer`:**
  - Initializes from an optional previous `CompactedSnapshot` and folds op chunks using the exact `rem-core` merge semantics:
    - Tombstones win; `DeckDelete` cascades removal of child cards and their review logs.
    - `DeckUpsert` applies iff `op.hlc > deck.hlc`.
    - `CardContentPatch` updates `front`/`back`/`tags` iff `op.hlc > card.content_hlc` (preserving `leech` tag commutation).
    - `CardSuspendPatch` updates `suspended` iff `op.hlc > card.suspended_hlc`.
    - `CardReview` / `CardReviewUndo` union `review_logs` by `id` (with `reviewLogTombstones` recording undone log IDs) and merge independent per-register HLC LWW fields (`op.hlc > scheduling_hlc`, `leech_hlc`, `suspended_hlc`) so reviews and undo operations commute cleanly.
  - Emits the next `CompactedSnapshot` JSON plus the live `referencedAssets` SHA-256 hash set across all surviving cards.
- **WASM build & freshness gate:**
  - Optional `wasm` Cargo feature using `wasm-bindgen` exposing streaming `ReducerHandle` (`from_snapshot_json`, `push_ops_chunk_json`, `finish_json`, `referenced_assets_json`).
  - `tools/build-sync-wasm.sh` builds `wasm32-unknown-unknown` with `wasm-bindgen --target web` into `sync-server/src/wasm/` so Workers statically `import wasmModule from './wasm/rem_sync_reduce_bg.wasm'` and call `initSync({ module: wasmModule })`.
  - Checked-in WASM artifacts keep `sync-server` CI and Cloudflare deploy workflows free of any Rust toolchain dependency; the `rust` CI job runs `tools/build-sync-wasm.sh --check` (verifying `source.sha256` against `rem-sync-reduce` sources/dependencies, compiling the WASM target, and diffing the generated JS/TS bindings byte-for-byte).

---

## 4. Slice 3 — Server Compaction, `/v1/bootstrap`, and Non-Destructive Client Merge

### Durable Object state & configuration

`UserSyncDO` adds a `sync_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)` table storing:
- `user_id` (persisted on first authenticated Worker RPC so background alarms can build R2 keys)
- `latest_seq` (monotonic high-water mark so pruning `ops` never regresses `seq` or `latestSeq`)
- `snapshot_seq`, `snapshot_r2_key`, `min_retained_seq`, `compaction_scheduled`

Environment knobs in `sync-server/wrangler.toml`:
- `COMPACT_AFTER_OPS` (default `5000`)
- `RETAIN_TAIL_OPS` (default `2000`)
- `RETAIN_TAIL_DAYS` (default `30`)
- `ENABLE_OP_PRUNING` (default `"true"` in `wrangler.toml`; set `"false"` to disable pruning)
- `ASSET_GC_GRACE_DAYS` (default `30`)

### Background `alarm()` compaction

1. When `latest_seq - snapshot_seq >= COMPACT_AFTER_OPS`, `UserSyncDO.sync()` schedules a DO alarm (idempotently guarded by `compaction_scheduled`).
2. In `alarm()`:
   - Load previous snapshot from R2 (if `snapshot_r2_key` is set) and gunzip via `DecompressionStream('gzip')`.
   - Instantiate `ReducerHandle` from the pre-initialized WASM module.
   - Stream `ops` in `(prevSnapshotSeq, targetSeq]` in chunks of 500–1,000 rows to bound Worker memory.
   - Finish reduction, gzip the resulting `CompactedSnapshot` via `CompressionStream('gzip')`, and write to an immutable R2 key `users/${userId}/snapshots/${targetSeq}.json.gz`.
   - Atomically update `sync_meta` (`snapshot_seq`, `snapshot_r2_key`, `compaction_scheduled`); if `ENABLE_OP_PRUNING === "true"`, compute `pruneSeq` respecting both `RETAIN_TAIL_OPS` and `RETAIN_TAIL_DAYS`, delete `ops WHERE seq <= pruneSeq`, and update `min_retained_seq`.
   - Best-effort delete the superseded R2 snapshot key after metadata commits.

### Bootstrap wire contract & client flow

- **`GET /v1/bootstrap`:** Authenticated via `Authorization: Bearer <jwt>` + entitlement check. Streams the current `snapshot_r2_key` from R2 with `Content-Type: application/json`, `Content-Encoding: gzip`, `X-Rem-Snapshot-Seq`, and `ETag` (returns `404` if no snapshot exists yet).
- **`POST /v1/sync` stale-cursor handling:** If `req.sinceSeq < minRetainedSeq` and a snapshot exists, `UserSyncDO` still inserts and ACKs any pushed `req.ops` first (never losing offline mutations), then returns `{ bootstrapRequired: true, snapshotSeq, ops: [], hasMore: false, latestSeq, ackedOpIds }`.
- **`rem-core::Collection::apply_bootstrap_snapshot`:** Merges `CompactedSnapshot` non-destructively using CRDT/HLC rules inside a single SQLite `write_transaction`, advances `last_seq = max(last_seq, snapshot.snapshot_seq)` and `last_hlc = max(last_hlc, snapshot.max_hlc)`, bumps `sync_revision` if mutated, and returns `missing_asset_hashes`.
- **`rem-sync` loop:**
  - Fresh device (`last_seq == 0`): calls `GET /v1/bootstrap` first (falling back gracefully on `404` to normal incremental replay from `0`), applies snapshot non-destructively, downloads missing assets, and continues normal `POST /v1/sync` from `snapshot_seq`.
  - Stale device (`bootstrapRequired == true`): ACKs any pushed local ops first, fetches & merges `/v1/bootstrap`, downloads missing assets, and continues incremental pull from `snapshot_seq`.

---

## 5. Slice 4 — Hibernatable WebSocket Pokes

### Server (`GET /v1/sync/ws` + `UserSyncDO`)

- **Auth & upgrade (`index.ts`):** Clients open a WebSocket to `GET /v1/sync/ws` with `Authorization: Bearer <jwt>` and `X-Rem-Device-Id: <deviceId>` headers (tokens are never placed in query parameters). The Worker verifies the JWT, enforces active sync entitlement (`402` if paused), and forwards the upgrade to `UserSyncDO`.
- **Hibernation (`UserSyncDO`):**
  - Accepts sockets via `this.ctx.acceptWebSocket(server, [deviceId])` and stores `{ deviceId }` via `server.serializeAttachment(...)`.
  - Registers `this.ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('ping', 'pong'))` so client keepalive pings never wake the Durable Object from hibernation.
  - Whenever `sync(req)` inserts `> 0` new ops, broadcasts `{"type":"poke","latestSeq":N}` to every connected socket whose attached `deviceId !== req.deviceId` (closing dead sockets cleanly).
  - On `DELETE /v1/account` (`deleteAllData()`), closes all active sockets with code `4001`.

### Desktop (`rem-sync` + `src-tauri/src/sync.rs` + `SyncContext.tsx`)

- `rem-sync` provides a reusable WebSocket poke listener (`poke.rs`, `tungstenite` + native/rustls TLS) plus `prepare_poke_auth` to supply `(server_url, device_id, fresh_access_token)` with automatic refresh-token rotation.
- `SyncManager` in `src-tauri/src/sync.rs` manages the background poke listener lifecycle: starts when Cloud sync is enabled, signed in, and unpaused; restarts on sign-in / account switch / trial unlock; stops immediately on sign-out, account deletion, or subscription pause (`402`).
- On receiving `{ type: "poke", latestSeq }` where `latestSeq > local.last_seq`, triggers `run_sync_locked` cleanly (single-flight serialized with manual/debounced syncs).
- Exposes `pokeConnected: boolean` on `SyncStatusSnapshot` (emitted via `sync:state-changed`). `SyncContext.tsx` backs off `PERIODIC_SYNC_MS` from 5 minutes to 30 minutes while `pokeConnected === true`, immediately returning to 5 minutes as fallback whenever disconnected.

### iOS (`SyncPokeClient.swift` + `AppState.swift` + `RemApp.swift` + `rem-ffi`)

- `rem-ffi` exposes `RemClient::prepare_poke_auth(now_ms, force_refresh) -> PokeAuthDto` so Swift obtains a valid refreshed access token, normalized server URL, and `deviceId` without duplicating refresh-token logic.
- `SyncPokeClient` uses native `URLSessionWebSocketTask` against `GET /v1/sync/ws` with `Authorization: Bearer <token>` and `X-Rem-Device-Id: <deviceId>` headers, sends text `"ping"` keepalives matching the DO auto-response pair, handles exponential reconnect backoff, and forces token refresh on auth failure.
- `AppState` + `RootContainerView` connect `SyncPokeClient` **only** while `scenePhase == .active` and Cloud sync is signed in and unpaused, disconnecting cleanly on `.inactive`/`.background`, sign-out, account deletion, or entitlement pause. Incoming pokes with `latestSeq > syncStatus.lastSeq` invoke `syncNow(silent: true)`.

---

## 6. Slice 5 — Grace-Period R2 Orphan Asset GC & Account Cleanup

- During each compaction alarm in `UserSyncDO`:
  - Collect live asset hashes from `ReducerHandle.referenced_assets_json()` (covering the compacted snapshot) **union** `asset_refs` extracted from any uncompacted tail ops (`seq > targetSeq`).
  - Query DO `assets` rows not in the live set whose `uploaded_at < now - ASSET_GC_GRACE_DAYS * 86_400_000` (default 30 days) so recently uploaded assets referenced by in-flight pushes are never prematurely collected.
  - Delete qualifying orphan objects from R2 and remove their rows from DO `assets`.
- `DELETE /v1/account` deletes both `users/${userId}/assets/*` and `users/${userId}/snapshots/*` prefixes in R2, closes active WebSocket sockets (`4001`), and wipes DO + D1 state.

---

## 7. Verification Strategy

- **Rust (`rem-sync-reduce`, `rem-core`, `rem-sync`, `rem-ffi`, `src-tauri`):**
  - Equivalence tests proving direct SQLite replay == `SnapshotReducer` + `Collection::apply_bootstrap_snapshot` (including deck-delete cascade, leech commutation, concurrent reviews, non-empty local collection, and offline outbox preservation).
  - Push-time outbox coalescing tests + `StudySession::undo_last_grade()` regression checks.
  - `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and WASM freshness (`tools/build-sync-wasm.sh --check`).
- **Sync server (`vitest-pool-workers`):**
  - Echo suppression + `hasMore` pagination, gzip decompression + 8 MB zip-bomb 413 guard, Hibernatable WS auth/entitlements/poke broadcast/no-echo/close-on-delete, alarm compaction with `ENABLE_OP_PRUNING` both `"false"` and `"true"`, stale-cursor `bootstrapRequired` preserving pushed ops, and grace-period orphan asset GC.
- **Web & iOS:**
  - `SyncContext` unit tests verifying 5 min ↔ 30 min periodic interval switching on `pokeConnected`.
  - iOS `RemAppTests` covering WS URL building, poke payload parsing, seq gating, and backoff logic, plus full `RemCore.xcframework` + Simulator `xcodebuild test`.
