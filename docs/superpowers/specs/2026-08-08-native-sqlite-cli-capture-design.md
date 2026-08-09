# Native SQLite collection + CLI capture — design

_Date: 2026-08-08_

## Context

rem is a native-only Tauri application whose production collection currently lives in Dexie/
IndexedDB inside the webview. That works well for the React UI, but an external command cannot
safely open the same browser-owned database. The next product slice is terminal card capture for
humans, scripts, and AI agents. A later terminal study interface should also work with the desktop
application closed.

The decisions behind this design are:

- The CLI must work while the desktop application is closed.
- Changes made outside the desktop UI do not need to appear live immediately in an already-open
  screen. Navigation, refresh, or a later focus refresh is sufficient initially.
- CLI writes do not trigger Git sync. The desktop application's next normal sync carries them.
- AI-authored cards should eventually enter a local draft-approval flow instead of being silently
  scheduled, but that is a follow-on slice.
- There is no installed-user migration requirement. The first SQLite release may start with a fresh
  collection and leave old IndexedDB data unused.

## Goal

Make one native collection available to the desktop UI and a non-interactive CLI, while preserving
rem's current backup and Git-sync formats and establishing the correct foundation for a later TUI.

Success means:

- SQLite is the only production collection store.
- `rem deck list` and `rem card add` work with the desktop app closed.
- The CLI and desktop read and write the same database through the same Rust collection module.
- Existing deck/card/review/settings/stats/assets behaviour remains intact in the desktop app.
- Git sync continues to exchange logical `rem-sync` JSON files and content-addressed assets; no
  SQLite files enter the repository.
- Agent-oriented commands accept multiline JSON/stdin, return stable JSON, insert batches
  atomically, and avoid duplicate inserts on ordinary retries.

## Architecture

```text
React UI ── TauriStorage adapter ──┐
                                  ├── Rust Collection module ── SQLite
CLI ───── command adapter ─────────┘
                                                  │
                                      logical RepoSnapshot
                                                  │
                                      existing TS merge/format
                                                  │
                                      JSON + assets in Git
```

### Shared Rust collection module

Add a small Rust crate under the existing `src-tauri` Cargo workspace. Its concrete `Collection`
module owns:

- database path resolution and schema migrations;
- connection configuration and transactions;
- Rust persistence models and serde wire models;
- deck/card/review/daily-stat/asset/tombstone operations;
- card-creation invariants such as UUIDs, timestamps, initial FSRS state, tag normalization, and
  atomic batch duplicate handling;
- logical snapshot export and merge-operation application.

The module does not expose raw SQL. Tauri commands and CLI commands are thin adapters over
intent-level operations. This is the seam that lets desktop, CLI, and a future TUI share behaviour
instead of merely sharing tables.

The existing Rust FSRS implementation may remain in the Tauri crate during the storage cutover. It
moves into the shared module only when the TUI study slice ports the complete review operation.
SQLite is necessary for a TUI, but it is not sufficient: queue selection, learning steps, daily
caps, leech effects, and grade commits must also be shared before terminal study ships.

### Cargo shape

Keep the Rust workspace rooted at `src-tauri` so existing build paths and target caching remain
local to the native project:

```text
src-tauri/
  Cargo.toml                 # Tauri app + workspace root
  crates/
    rem-core/                # Collection and, later, shared review logic
    rem-cli/                 # CLI/TUI binary adapter
```

The Tauri application and `rem-cli` both depend on `rem-core`. CI runs format, Clippy, and tests for
the whole workspace. Release packaging of the terminal binary is a gated task after the command
interface is stable; its installed command is `rem`, even if the internal Cargo package remains
`rem-cli`.

## SQLite design

Use `rusqlite` with bundled SQLite for consistent macOS/Linux/Windows builds. The database lives at
a shared OS-appropriate path resolved by `rem-core`; tests open explicit temporary paths.

Initial schema:

```text
metadata       schema/sync revision values
decks          identity, timestamps, name/color/kind, settings JSON
cards          identity, deck, timestamps, front/back, tags JSON, state flags,
               scheduling JSON, indexed due time
review_logs    immutable grades
assets         content-addressed bytes and MIME type
tombstones     synchronized deletions
daily_stats    local per-deck/day counters
```

Settings and scheduling remain JSON payloads because they evolve as domain values. Fields needed for
queries or integrity (`deck_id`, `due`, `updated_at`, `suspended`) are explicit indexed columns.
Writes update the scheduling JSON and indexed due column in the same transaction.

Enable foreign keys, WAL mode, a bounded busy timeout, and short transactions. SQLite serializes
physical writes; domain operations must still protect against stale logical writes.

## Desktop storage cutover

Add `TauriStorage`, implementing the existing TypeScript `Storage` interface through typed Tauri
commands. The React feature modules continue to depend on `Storage`, not on SQL or Rust details.

Dexie remains only as a browser-test adapter while those tests need an in-process store. It is no
longer selected in a packaged Tauri application. Production must never split data between Dexie and
SQLite.

The UI currently uses `dexie-react-hooks` for initial reads and live invalidation. Replace this in
feature code with a store-agnostic query hook. Storage adapters notify that hook after mutations made
through the current renderer. This preserves immediate updates for normal desktop interactions.
External CLI writes do not emit renderer events; an open screen may remain stale until navigation or
focus, which is accepted for this slice.

There is no IndexedDB-to-SQLite importer. Existing development IndexedDB data may be cleared or
ignored. Backup/import remains the explicit portable user format.

## Git sync interaction

SQLite is local operational state, not the sync file format. Keep the current protocol:

1. Fetch/reset the disposable Git working copy.
2. Deserialize remote JSON/assets into `RepoSnapshot`.
3. Export the local logical snapshot from SQLite.
4. Run the existing pure TypeScript LWW merge.
5. Apply merge operations to SQLite transactionally.
6. Serialize the merged snapshot and push it.

The database file, WAL, and shared-memory files live outside `app-data/repo` and must never be added
to Git. The `rem-sync` version does not change merely because the local schema changes.

### Concurrent local writers during sync

CLI/TUI access creates a real second writer. Prevent a stale export/merge/apply cycle from
silently replacing a newer local edit:

- Maintain a monotonic `sync_revision` in SQLite for changes to synchronized records.
- Snapshot export returns `{ snapshot, revision }`.
- Applying merge operations succeeds only when the observed revision still matches.
- If it changed, `GitSyncService` reloads local state and recomputes against the already-fetched
  remote snapshot.
- A write after a successful local apply is retained locally and joins the next sync; no database
  lock is held during Git network operations.

Later TUI study also needs per-card optimistic concurrency on grade commits so the GUI and TUI
cannot grade the same stale card twice. That is part of the TUI review-engine slice, not basic card
capture.

## CLI interface — first slice

Human-friendly single-card input:

```sh
rem deck list
rem card add --deck <id-or-exact-name> \
  --front-file question.md \
  --back-file answer.md \
  --tag rust --tag ownership
```

Agent-friendly batch input and machine output:

```sh
rem card add --deck <deck-id> --input-json - --output json <<'JSON'
[
  {
    "front": "What does Rust's `move` keyword do?",
    "back": "It transfers ownership of captured values.",
    "tags": ["rust", "ownership"]
  }
]
JSON
```

Interface rules:

- A deck reference may be an ID or exact name. An ambiguous name is an error that lists candidate
  IDs.
- Front must contain non-whitespace; back may be empty to match the desktop editor.
- Markdown is stored exactly. Tags are trimmed, case-insensitively de-duplicated, and may not set
  system-owned tags such as `leech`.
- A JSON batch is one transaction: validation or insertion failure creates no cards.
- Exact same deck/front/back/tags content is treated as an existing card by default and reported as
  `duplicate`; an explicit flag permits intentional duplicates.
- `--dry-run` resolves the deck and validates/deduplicates without writing.
- `--output json` writes only one versioned result object to stdout. Diagnostics go to stderr and
  failures use stable non-zero exit codes.
- Images/asset ingestion, card editing/deletion, deck creation, review, and Git sync are out of the
  first CLI slice.

## Follow-on: local draft approval

Generated content should use a separate local `card_drafts` table rather than suspended cards or a
special tag. A draft has target deck, Markdown front/back, tags, and creation metadata, but no
scheduling state, due count, review history, backup entry, or Git-sync representation.

Proposed flow:

```text
agent: rem draft add ...
               ↓
local draft inbox
               ↓
human edits / accepts / rejects
               ↓
accept transaction creates a normal card and removes the draft
```

The agent skill should default to drafts once this flow exists; direct `card add` remains for
trusted scripts. Draft persistence, CLI approval, and a desktop Inbox are a separate user-visible
slice after direct CLI capture is validated.

## Follow-on: TUI study

`rem study` should not reimplement the TypeScript review flow. Before it ships:

- move Rust FSRS commands into `rem-core`;
- port learning/relearning transitions, due-session construction, caps, leech effects, and atomic
  grade commits behind a shared review interface;
- run parity fixtures against the current TypeScript behaviour during the transition;
- make the React review screen and terminal UI adapters over the same shared operations;
- add per-card optimistic grade commits;
- render Markdown/code in the terminal, with explicit placeholders for unsupported images.

This is intentionally planned after CLI capture and draft approval so terminal interaction does not
expand the storage cutover.

## Out of scope

- Syncing raw SQLite databases through Git.
- Triggering sync from CLI commands.
- Live cross-process UI notifications.
- IndexedDB migration.
- Browser product support.
- Draft synchronization across devices.
- TUI review implementation in the initial CLI slice.
- Moving Git merge logic to Rust before a headless `rem sync` requirement exists.

## Verification

- Rust integration tests run every collection operation against temporary SQLite databases.
- Two independent `Collection` instances prove app/CLI visibility and write serialization.
- Shared serialization fixtures verify Rust wire models match TypeScript models.
- Existing TypeScript unit/browser tests remain green against the injected test adapter.
- Git-sync tests prove its file format is unchanged and local-revision retries preserve concurrent
  writes.
- CLI process tests cover deck resolution, multiline Markdown, atomic batches, duplicates, dry-run,
  JSON output, and failures.
- Manual native smoke: create/edit/review/import/sync in the desktop app, add a card while it is
  closed through CLI, reopen, and verify the card is due and later syncs normally.
