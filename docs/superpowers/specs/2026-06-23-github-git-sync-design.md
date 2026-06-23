# GitHub / Git-Backed Sync — Design

_Date: 2026-06-23_

## Goal

Sync rem's data across machines and let it be reused on multiple instances, using **git as
the backend storage** and **GitHub as the remote**. rem becomes a **Tauri desktop app** that
shells out to the **system `git`**, reusing the user's existing git credentials. rem stores no
token and holds no secret.

## Decisions (and why)

| Decision | Choice | Why |
|---|---|---|
| Git's role | Sync/backup **remote**; IndexedDB stays the live working store | Smallest change; builds on the existing `Storage` seam and backup format. |
| Target + auth | **Tauri desktop app + local git auth** | A browser has no trusted local process, so any token is reachable by same-origin JS. A native app shells out to system `git` and reuses existing SSH / credential-manager auth — **no token in rem at all**. This is literally "git as the backend." |
| Reconciliation | **Per-record last-writer-wins (LWW)** merge by timestamp | Reviews of *different* cards on two machines both survive; only the *same* card edited on both sides picks the newer. Correct for multi-device. |
| Repo layout | **File per deck**, extensible | Readable per-deck history, low file count, matches deck-as-namespace; reserves room for a future review log. |
| Deletions | **Tombstones** | Without them, a card deleted on one machine resurrects after syncing with another. Tombstones make deletes stick. |
| Sync trigger | **Auto on launch/quit + manual "Sync now"** | Seamless across instances without per-keystroke churn. |
| Repo setup | **rem owns a clone from a remote URL** | Self-contained; uses existing git auth; the user never touches the folder. |

## Architecture

Two layers with a strict split of responsibility:

- **Renderer (existing React app, essentially unchanged).** Dexie/IndexedDB stays the live
  working store. **All merge intelligence lives here, in pure, testable TypeScript.** We add a
  `GitSync` service and a Settings panel.
- **Rust / Tauri backend — a dumb transport.** Shells out to system `git`, reusing existing
  auth. Exposes a handful of narrow commands (clone, fetch-reset, read files, write files,
  commit-and-push). No application logic.

**Prerequisite:** system `git` must be installed. rem detects its absence and shows a clear
setup message rather than failing cryptically.

## The sync protocol

The DB is the source of truth on each machine, so the git working copy is treated as disposable
scratch space. This design means **git never produces conflict markers** — the app does all
merging.

1. `git fetch` the remote.
2. `git reset --hard origin/main` → working copy now mirrors the remote exactly. _(Also handles a
   previous failed/offline push: any un-pushed commit is discarded, but its data still lives in
   the DB, so it simply re-merges next.)_
3. Read repo files → **remote snapshot**. Read DB → **local snapshot**.
4. Run the pure **LWW merge** (per-card newest-wins by `updatedAt`; tombstones beat older edits)
   → authoritative **merged state** + `dbOps`.
5. Apply merged state to the **local DB** (upserts + tombstone deletes).
6. Write merged state back to the working-copy files.
7. `git add -A`; commit if anything changed; `git push`.
8. **If push is rejected** (another device pushed during the window) → loop back to step 1. The
   merge is convergent, so this terminates (bounded retries).

**First sync / empty remote:** if `origin/main` does not exist, treat the remote snapshot as
empty, create the initial commit, and push to create `main`.

## Components

### Renderer (TypeScript)

- `src/data/sync/snapshot.ts` — `RepoSnapshot` type + pure (de)serialization to the file-per-deck
  layout. Owns the filename ↔ record mapping.
- `src/data/sync/merge.ts` — **pure** `merge(local, remote) → { merged, dbOps }`. LWW + tombstone
  core. No I/O. The most heavily tested file.
- `src/data/sync/GitSyncService.ts` — orchestrates the 8-step protocol; owns push-reject retry and
  empty-remote bootstrap.
- `src/data/sync/GitBridge.ts` — interface wrapping the Tauri commands (`clone`, `fetchReset`,
  `readFiles`, `writeFiles`, `commitPush`). Real impl calls Tauri; tests use an in-memory fake.
  Also the seam where a future web/HTTP backend could slot in (not built now).
- **Storage additions:** `exportSnapshot()` and `applyMerge(dbOps)`; `deleteDeck`/`deleteCard`
  now write a tombstone.
- **Settings UI:** remote-URL field, "Sync now" button, last-sync time + readable status/errors.

### Data model changes (small)

- New Dexie **`tombstones` table**: `{ id, kind: 'deck' | 'card', deletedAt }`.
- **Dexie v3 migration** — additive, no data rewrite.
- IDs (`crypto.randomUUID()`) and `updatedAt` already exist; no change. `Deck.updatedAt` is
  deferred until a rename feature exists — decks merge by union-on-id today.

### Repo file format (`rem-sync`, version 1)

```
rem.json                 # { format:"rem-sync", version:1, ... } — manifest
decks/<deckId>.json      # { deck:{id,name,createdAt,schedulerKind}, cards:[...] }
tombstones.json          # [ {id, kind, deletedAt}, ... ]
# reviews/  ← reserved for a future append-only stats log (not built now)
```

This is a **new format**, separate from the existing export `backup.ts` format (which drops IDs
and replaces-by-name — unsuitable for sync).

### Rust / Tauri

- Scaffold Tauri to load the existing Vite build.
- Implement ~5 narrow git commands by shelling out to `git`.
- Grant access to the app-data dir where the clone lives.
- No app logic in Rust.

## Error handling

Surfaced as readable Settings messages; never crashes the app:

- **git not installed** → clear setup message.
- **auth/permission failure** → show git's stderr.
- **offline / unreachable remote** → keep working locally; DB unaffected.
- **empty remote on first sync** → bootstrap an initial commit.
- **push race** → auto-retry (protocol step 8).

**Known limitation (documented):** LWW assumes roughly-synced clocks — acceptable for personal
use. Could add logical counters later.

## Testing

- **`merge.ts` — exhaustive unit tests:** different cards edited on each side both survive; same
  card → newer `updatedAt` wins; tombstone newer than an edit → stays deleted; tombstone older
  than a re-creation → record returns; new deck on one side; empty-vs-populated.
- **`snapshot.ts`** — round-trip serialize→deserialize fidelity (incl. tombstones + scheduling).
- **`GitSyncService`** — full protocol against an in-memory fake `GitBridge`: push-race retry,
  empty-remote bootstrap, offline error path. No real git/Tauri.
- **Rust git commands** — light integration tests against a temp local bare repo (clone / commit /
  push / fetch-reset). Kept light because Rust holds no logic.
- **Existing suite stays green** — all sync code is additive; the bridge wires to Tauri only in the
  desktop entry, so the web tests are untouched.

## Out of scope (YAGNI — documented as future)

- OAuth / any token storage (local git auth replaces it).
- A web-target sync backend (desktop-only now; `GitBridge` keeps the door open).
- Review-log / stats sync (format reserves `reviews/`; not built).
- Conflict-resolution UI (LWW is automatic; git history is the recovery path).
- Tombstone garbage-collection (keep forever for MVP).
- Code-signing / auto-update pipeline (separate track; dev build is fine to start).

## Implementation sequencing

- **Phase 0 — Tauri shell** that runs the *existing* app unchanged. _Verify: app launches and
  works in the desktop window._
- **Phase 1 — pure core:** `merge` + `snapshot` + tombstones table/migration. _Verify: unit tests._
- **Phase 2 — transport:** `GitBridge` + Rust commands + `GitSyncService`. _Verify: fake-bridge
  protocol tests, then real git against a scratch repo._
- **Phase 3 — UI + wiring:** Settings panel + launch/quit auto-sync. _Verify: end-to-end
  clone → edit → sync → pull on a second clone._
