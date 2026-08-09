# Native SQLite collection + CLI capture — implementation plan

_Date: 2026-08-08_

**Goal:** Replace Dexie as rem's production store with a shared native SQLite collection and ship a
reliable CLI that can add Markdown cards while the desktop app is closed.

**Architecture:** `rem-core` owns SQLite, persistence models, invariants, and transactions. The
Tauri UI and `rem-cli` are adapters over that module. TypeScript keeps the existing `Storage` seam
and pure Git merge/serialization. Git continues to sync logical JSON/assets, never the database.

**Execution rule:** Complete and verify one batch before starting the next. Do not begin draft
approval or TUI study as part of the storage/CLI batches; each receives a focused follow-on design
check after real CLI use.

## Global constraints

- No IndexedDB migration. Packaged Tauri builds select SQLite only.
- Do not modify or add the untracked `features.md` or
  `docs/CURRENT_STATE_AND_FUTURE_VISION.md` while executing this plan.
- Keep `rem-sync` and backup wire formats backward compatible.
- Use test-first changes for collection operations, adapter contracts, concurrency, and CLI
  behaviour.
- No raw SQL crosses the `rem-core` interface.
- No Git command runs as a side effect of card capture.
- Preserve Markdown byte-for-byte; normalize only validation and tags.
- Run Rust checks for the whole workspace after the workspace is introduced.

---

## Batch 1 — Rust workspace and SQLite foundation

**Outcome:** A tested shared collection can open the same native database from independent
processes, but the desktop app still uses Dexie.

### 1.1 Establish the workspace

- Make `src-tauri/Cargo.toml` the app package plus workspace root.
- Add `src-tauri/crates/rem-core` and `src-tauri/crates/rem-cli` skeleton crates.
- Add `rem-core` as a path dependency of the Tauri app and CLI.
- Update CI/cache commands to run `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- Keep release/version automation targeting the existing Tauri app package; add CLI version wiring
  only in the packaging batch.

**Verify:** all pre-existing Rust tests and the empty CLI smoke test pass through workspace commands.

### 1.2 Create the collection schema

- Add `rusqlite` with bundled SQLite, serde/serde_json, UUID, hashing, and temporary-directory test
  dependencies at the narrowest crates that need them.
- Implement explicit database-path resolution shared by app and CLI; tests always pass a temp path.
- Add schema v1 with `metadata`, `decks`, `cards`, `review_logs`, `assets`, `tombstones`, and
  `daily_stats`.
- Store settings/scheduling/tags as JSON while indexing `deck_id`, `due`, `updated_at`, and
  `suspended`.
- Enable foreign keys, WAL for file databases, and a bounded busy timeout.
- Add schema-version rejection for databases newer than the binary understands.

**Test first:** schema creation, reopen, constraints/indexes, unsupported newer schema, and two
connections to one temp database.

### 1.3 Define compatible Rust models

- Mirror current deck, settings, card, FSRS state, review log, tombstone, asset, backup-import, and
  sync-operation payloads with camelCase serde names.
- Add JSON fixture tests proving Rust reads/writes representative payloads from the current
  TypeScript model and `rem-sync` shape.
- Put initial FSRS card construction in `rem-core` so GUI and CLI create identical due-now cards.
- Put user-tag normalization in `rem-core`; preserve `leech` as system-owned metadata.

**Batch gate:** `cargo fmt --all --check`, workspace Clippy, and workspace tests are green. Review the
schema before implementing all operations; changing it is cheapest here.

---

## Batch 2 — Complete native collection parity

**Outcome:** `rem-core::Collection` can satisfy every production operation currently provided by
`DexieStorage`; no frontend cutover yet.

### 2.1 Deck and card operations

Implement test-first:

- create/list/get/update/delete deck;
- create-one and create-batch cards;
- get/list/update/delete card;
- due-card query and due count;
- tombstone creation and deletion cascades;
- atomic import with replace-by-exact-name semantics.

Creation owns UUIDs/timestamps/initial state and verifies the target deck exists. Batch creation
validates all inputs before writing any row. Exact duplicate detection runs inside the same
transaction.

### 2.2 Review, counters, and assets

Implement test-first:

- atomic review commit, optional immutable review log, and daily-counter increment;
- list review history;
- read daily counters;
- content-addressed asset insert/get;
- orphan-asset sweep.

Test rollback on failures and ensure scheduling JSON and indexed due time cannot diverge.

### 2.3 Backup and sync operations

Implement test-first:

- logical `RepoSnapshot` export;
- transactional application of merge operations;
- review-log and asset upsert/delete behaviour;
- all current snapshot fields without adding SQLite-local columns to the wire format.

Add `sync_revision` and make synchronized mutations increment it transactionally. Export returns the
observed revision; merge application returns a stale-revision result rather than overwriting state
when another writer changed synchronized data.

### 2.4 Cross-connection verification

- Open two `Collection` instances against one temporary file.
- Write through one and immediately read through the other.
- Exercise concurrent batch inserts and bounded lock waiting.
- Prove a stale merge revision is rejected and leaves the newer local write intact.

**Batch gate:** workspace Rust checks green; operation coverage includes every current `Storage`
method and rollback path. Do not wire the UI until this parity list is complete.

---

## Batch 3 — Tauri adapter and desktop cutover

**Outcome:** The packaged desktop app uses SQLite exclusively and retains current visible behaviour.

### 3.1 Manage the collection in Tauri

- Open the shared collection during Tauri setup and fail startup with a readable database error.
- Register typed commands for the collection operations. Commands are thin adapters: no duplicated
  normalization, transaction, or scheduling-initialization logic.
- Keep the Git working copy separate from the collection path.
- Add Rust command-level tests where argument/result conversion adds behaviour beyond `rem-core`.

### 3.2 Add `TauriStorage`

- Implement the TypeScript `Storage` interface using typed `invoke` calls.
- Map Rust byte arrays to `Uint8Array` at the asset seam.
- Add adapter tests with an injected invoke function rather than a running Tauri process.
- Select `TauriStorage` in packaged/native execution. Keep `DexieStorage` only for browser tests and
  explicitly unsupported non-Tauri development fallback.

### 3.3 Replace Dexie-specific reactivity

- Add a store-agnostic storage query/invalidation mechanism.
- Storage adapters notify subscribers after renderer-originated mutations.
- Replace feature-level `useLiveQuery` usage without changing screen behaviour.
- Do not add cross-process file watching or polling. CLI changes may require navigation/refresh to
  appear in an already-open screen.
- Remove `dexie-react-hooks` only if no test adapter usage still requires it; do not force unrelated
  test rewrites merely to remove a dependency.

### 3.4 Desktop parity verification

Automated:

- Run all unit and browser tests.
- Add focused tests for GUI mutation invalidation after create/update/delete/import/sync.
- Run frontend typecheck/build and all workspace Rust gates.

Manual native smoke:

1. Create and edit a deck/card with tags.
2. Add an image, save, reopen, and render it.
3. Complete learning and normal reviews; verify due counts, stats, and optimization history.
4. Export and re-import a backup.
5. Restart the app and verify persistence.

**Batch gate:** SQLite is the sole packaged store and the complete desktop smoke passes. No CLI
feature claim yet.

---

## Batch 4 — Git-sync concurrency adaptation

**Outcome:** Existing Git sync works unchanged at the wire level and cannot erase a concurrent local
CLI/TUI write.

### 4.1 Version the local sync read/apply contract

- Change the storage sync interface to export `{ snapshot, revision }`.
- Apply merge operations only against the expected revision.
- On a stale result, have `GitSyncService` re-export and recompute against the already-fetched remote
  state with a small bounded local retry.
- Do not hold a SQLite transaction or lock while fetching/pushing Git.
- Preserve existing remote push-rejection retries as a separate loop.

The Dexie test adapter may implement the revision contract for in-process tests, but it must not be
selected in production.

### 4.2 Preserve format and merge semantics

- Keep `SYNC_VERSION`, file names, JSON payloads, assets, tombstones, and LWW rules unchanged.
- Add a regression test that serialized files before and after the storage cutover are equivalent.
- Add a concurrent-writer service test: mutate an existing local card between export and apply;
  verify sync retries and preserves the new edit.
- Repeat existing remote deletion, review-log, asset, and push-race tests.

### 4.3 Native sync smoke

- Sync SQLite data to a scratch bare Git repository.
- Add a card through a second collection connection during a sync cycle; verify it remains local and
  joins the next sync.
- Verify the database/WAL files are outside and absent from the Git repository.

**Batch gate:** TypeScript tests/build and workspace Rust gates green; manual scratch-repo sync
passes. CLI still does not trigger sync.

---

## Batch 5 — Direct CLI capture

**Outcome:** Humans and agents can inspect decks and add cards atomically while the GUI is closed.

### 5.1 Lock the command contract

Implement with `clap` and a small presentation layer over `rem-core`:

```text
rem deck list [--output text|json]
rem card add --deck <id-or-exact-name> <single-card flags>
rem card add --deck <id-or-exact-name> --input-json <path|->
             [--dry-run] [--allow-duplicate] [--output text|json]
```

- Single-card flags support literal values and `--front-file` / `--back-file`; mutually exclusive
  forms fail clearly.
- JSON accepts one card or an array and preserves multiline Markdown.
- Name lookup errors on ambiguity and reports candidate IDs.
- Front is nonblank; back may be empty; tags use core normalization.
- Batch insertion is all-or-nothing.
- Exact duplicate content returns existing IDs/status by default; intentional duplicates require
  `--allow-duplicate`.
- JSON output has a versioned top-level object and no human diagnostics on stdout.

### 5.2 Test as a real process

Add CLI process tests for:

- empty and populated deck lists;
- lookup by ID and exact unique name;
- ambiguous/missing decks;
- literal, file, and stdin Markdown;
- atomic multi-card creation;
- duplicate retry and override;
- dry-run leaving the database unchanged;
- stable JSON success/error output and exit codes;
- one collection process writing and a second process reading the result.

Use an explicit test data path/environment override; never touch the developer's real collection.

### 5.3 Agent usage document

- Document the JSON input/result schema and examples.
- Document that capture is local and reaches Git only on the app's next sync.
- Add a minimal agent skill only after commands and error messages are stable. The skill should teach
  card-writing quality and use machine-readable commands; it must not contain persistence logic.

### 5.4 Package the terminal binary

Treat packaging as a gate, not an afterthought:

- Build the CLI for the same release targets as the app.
- Publish archives whose installed command is `rem`.
- Resolve the existing Linux AppImage command-name collision before changing `install.sh`; keep the
  GUI artifact and CLI executable at distinct paths.
- Install the macOS CLI into a user PATH location separately from `/Applications/rem.app`.
- Publish a Windows CLI archive; PATH automation may remain documented/manual initially.
- Update release CI and version automation so app/core/CLI versions stay aligned.

**Batch gate:** clean-machine smoke on each supported platform or CI runner: install, list decks,
add through stdin while the app is closed, reopen the app, review the due card, then sync it through
the existing GUI action.

---

## Follow-on Batch 6 — Local draft inbox (new focused slice)

Before implementation, write a short draft-inbox spec using feedback from direct CLI capture.
Expected scope:

- local-only `card_drafts` table;
- `rem draft add/list/accept/reject`;
- atomic accept-to-card operation;
- desktop Inbox with edit/accept/reject;
- agent skill switches from direct card creation to drafts by default;
- no scheduling, due counts, backup, assets, or Git sync before acceptance.

This is more elaborate than direct insertion because it adds a lifecycle and UI, but it is isolated
from scheduling and sync when drafts remain local-only.

## Follow-on Batch 7 — Shared review engine + `rem study` (new focused slice)

Do not build a TUI directly against raw card rows. First design and port the complete review
operation into `rem-core`:

- FSRS calculations and learning/relearning transitions;
- queue construction, insertion order, caps, and learn-ahead;
- leech effects and immutable logs;
- atomic grade commit with expected card row version;
- parity fixtures against current TypeScript behaviour;
- React review adapter cutover.

Then add a terminal adapter for due study with Markdown/code rendering, reveal/grade keys, progress,
and explicit image placeholders. Custom-study modes and rich terminal image protocols are separate
follow-ups unless the focused TUI spec explicitly includes them.

---

## Final verification for the SQLite/CLI program

```sh
npm test
npm run build
(
  cd src-tauri
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
)
```

Also complete:

- native desktop parity smoke;
- scratch Git remote sync smoke;
- CLI clean-data-dir process smoke;
- review of changed files for accidental IndexedDB production use or SQLite files under the Git
  working copy;
- documentation update only after the shipped batch accurately matches it.
