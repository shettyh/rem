# GitHub / Git-Backed Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make rem a Tauri desktop app that syncs its decks/cards across machines using a GitHub repo as the backend, via the system `git` (no token stored).

**Architecture:** IndexedDB (Dexie) stays the live store. A pure TypeScript layer (`snapshot` + `merge`) does per-record last-writer-wins reconciliation with tombstones. A thin Rust/Tauri bridge shells out to system `git` as dumb transport; all merge logic lives in TS. The sync protocol resets the working copy to the remote, merges in-app, writes back, and pushes — so git never produces conflict markers.

**Tech Stack:** React 19 + TypeScript + Vite · Dexie 4 · Tauri v2 (Rust) · Vitest (unit + Playwright browser) · system `git`.

## Global Constraints

- **TypeScript strict** — no `any` leaks; match existing code style (2-space indent, no semicolons, single quotes).
- **Surgical changes** — touch only what each task lists; do not refactor adjacent code.
- **Tauri v2** — `@tauri-apps/cli@^2`, `@tauri-apps/api@^2`, `tauri = "2"` in Cargo.
- **Prerequisite at runtime:** system `git` must be on PATH. Absence must produce a readable error, never a crash.
- **Sync wire format:** `{ format: "rem-sync", version: 1 }`, file-per-deck layout (see Task 4).
- **Existing test suite must stay green:** `npm test` runs `unit` + `browser` projects. Run single unit files with `npx vitest run <path>`.
- **Commit after every task** (frequent commits). Commit message style: `feat:` / `chore:` / `test:` prefixes, matching git history.
- **Reconciliation rule:** per-card LWW by `updatedAt`; a tombstone with `deletedAt` strictly greater than a card's `updatedAt` deletes it; deck tombstones delete the deck and cascade to its cards.

---

## File Structure

**New (TypeScript):**
- `src/data/sync/snapshot.ts` — wire types (`DeckRecord`, `CardRecord`, `RepoSnapshot`) + serialize/deserialize to the file-per-deck layout.
- `src/data/sync/snapshot.test.ts`
- `src/data/sync/merge.ts` — pure `merge(local, remote) → { merged, dbOps }`.
- `src/data/sync/merge.test.ts`
- `src/data/sync/GitBridge.ts` — `GitBridge` interface + result types.
- `src/data/sync/FakeGitBridge.ts` — in-memory bridge for tests.
- `src/data/sync/GitSyncService.ts` — the 8-step protocol orchestrator.
- `src/data/sync/GitSyncService.test.ts`
- `src/data/sync/TauriGitBridge.ts` — real bridge calling Tauri commands.
- `src/features/settings/SyncSection.tsx` — sync UI.
- `src/features/settings/SyncSection.browser.test.tsx`
- `src/app/useAutoSync.ts` — launch/visibility auto-sync hook.

**New (Rust/Tauri):** `src-tauri/` (scaffolded by `tauri init`), with `src-tauri/src/git.rs` added.

**Modified:**
- `src/domain/models.ts` — add `Tombstone`.
- `src/data/dexie/db.ts` — add `tombstones` table + v3 migration.
- `src/data/Storage.ts` — add `exportSnapshot`, `applyMerge`.
- `src/data/dexie/DexieStorage.ts` — implement them; `deleteDeck`/`deleteCard` write tombstones.
- `src/features/settings/SettingsPage.tsx` — render `<SyncSection />`.
- `src/app/main.tsx` — call `useAutoSync` (desktop only).
- `vite.config.ts` — Tauri-friendly dev server.
- `package.json` — Tauri deps + scripts.

---

## Phase 0 — Tauri shell (gated milestone)

### Task 1: Scaffold Tauri and run the existing app in a desktop window

**Files:**
- Create: `src-tauri/**` (via `tauri init`)
- Modify: `vite.config.ts`, `package.json`

- [ ] **Step 1: Install the Tauri CLI**

```bash
npm install -D @tauri-apps/cli@^2
npm install @tauri-apps/api@^2
```

- [ ] **Step 2: Scaffold `src-tauri` non-interactively**

```bash
npx tauri init --ci \
  --app-name rem \
  --window-title rem \
  --frontend-dist ../dist \
  --dev-url http://localhost:5173 \
  --before-dev-command "npm run dev" \
  --before-build-command "npm run build"
```

Expected: a `src-tauri/` directory containing `Cargo.toml`, `tauri.conf.json`, `build.rs`, `src/main.rs`, `src/lib.rs`, `capabilities/`, `icons/`.

- [ ] **Step 3: Make the Vite dev server Tauri-friendly**

Edit `vite.config.ts` — add `clearScreen` and `server` at the top level of the config object (siblings of `plugins` and `test`):

```ts
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  test: {
    // ...unchanged...
  },
})
```

- [ ] **Step 4: Add Tauri scripts to `package.json`**

Add to `"scripts"`:

```json
"tauri": "tauri",
"app:dev": "tauri dev",
"app:build": "tauri build"
```

- [ ] **Step 5: Run the desktop app**

Run: `npm run app:dev`
Expected: a native window opens showing rem's deck list; creating a deck and a card works exactly as in the browser (IndexedDB persists). Close the window to stop.

- [ ] **Step 6: Verify the web build is untouched**

Run: `npm test`
Expected: all existing tests still pass.

- [ ] **Step 7: Add `src-tauri/target` to gitignore and commit**

Append to `.gitignore`:

```
src-tauri/target/
```

```bash
git add .gitignore vite.config.ts package.json package-lock.json src-tauri
git commit -m "chore: wrap rem as a Tauri v2 desktop app"
```

> **MILESTONE GATE:** Do not start Phase 1 until the desktop app launches and the existing suite is green.

---

## Phase 1 — Data model + pure core

### Task 2: Add `Tombstone` model, tombstones table, and v3 migration

**Files:**
- Modify: `src/domain/models.ts`
- Modify: `src/data/dexie/db.ts`
- Test: `src/data/dexie/migration.test.ts` (add a case) — if unsure of its structure, create `src/data/dexie/tombstones.test.ts` instead with the test below.

**Interfaces:**
- Produces: `Tombstone = { id: string; kind: 'deck' | 'card'; deletedAt: number }`; `RemDB.tombstones` table keyed by `id`.

- [ ] **Step 1: Write the failing test**

Create `src/data/dexie/tombstones.test.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'

const DB_NAME = 'rem-tombstones-test'
let db: RemDB

beforeEach(async () => {
  await Dexie.delete(DB_NAME)
  db = new RemDB(DB_NAME)
})
afterEach(() => db.close())

describe('tombstones table', () => {
  it('stores and reads a tombstone by id', async () => {
    await db.tombstones.put({ id: 'card-1', kind: 'card', deletedAt: 42 })
    const t = await db.tombstones.get('card-1')
    expect(t).toEqual({ id: 'card-1', kind: 'card', deletedAt: 42 })
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/data/dexie/tombstones.test.ts`
Expected: FAIL — `db.tombstones` is undefined.

- [ ] **Step 3: Add the `Tombstone` type**

Append to `src/domain/models.ts`:

```ts
/** Records that a deck or card was deleted, so the deletion propagates on sync. */
export interface Tombstone {
  id: ID
  kind: 'deck' | 'card'
  /** When the deletion happened (epoch ms). */
  deletedAt: number
}
```

- [ ] **Step 4: Add the table + v3 migration in `db.ts`**

In `src/data/dexie/db.ts`, import the type and add the field + version. Final file:

```ts
import Dexie, { type EntityTable } from 'dexie'
import type { Card, Deck, Tombstone } from '../../domain/models'

/** IndexedDB schema. Indexed fields are listed; payloads are stored whole. */
export class RemDB extends Dexie {
  decks!: EntityTable<Deck, 'id'>
  cards!: EntityTable<Card, 'id'>
  tombstones!: EntityTable<Tombstone, 'id'>

  constructor(name = 'rem') {
    super(name)
    this.version(1).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
    })
    this.version(2)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
      })
      .upgrade(async (tx) => {
        await tx.table('decks').toCollection().modify((d) => {
          if (!d.schedulerKind) d.schedulerKind = 'sm2'
        })
        await tx.table('cards').toCollection().modify((c) => {
          if (c.scheduling && !c.scheduling.kind) c.scheduling.kind = 'sm2'
        })
      })
    // v3: add the tombstones table for deletion sync. Additive — existing data untouched.
    this.version(3).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
    })
  }
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `npx vitest run src/data/dexie/tombstones.test.ts`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/domain/models.ts src/data/dexie/db.ts src/data/dexie/tombstones.test.ts
git commit -m "feat: tombstones table + v3 migration for deletion sync"
```

---

### Task 3: Snapshot wire format (serialize / deserialize)

**Files:**
- Create: `src/data/sync/snapshot.ts`
- Test: `src/data/sync/snapshot.test.ts`

**Interfaces:**
- Consumes: `SchedulerKind`, `SchedulingState`, `Tombstone` from `../../domain/models`.
- Produces: `DeckRecord`, `CardRecord`, `RepoSnapshot`; `EMPTY_SNAPSHOT`; `serializeSnapshot(snap): Record<string,string>`; `deserializeSnapshot(files): RepoSnapshot`; constants `SYNC_FORMAT`, `SYNC_VERSION`.

- [ ] **Step 1: Write the failing test**

Create `src/data/sync/snapshot.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import {
  serializeSnapshot,
  deserializeSnapshot,
  EMPTY_SNAPSHOT,
  type RepoSnapshot,
} from './snapshot'

const sample: RepoSnapshot = {
  decks: [{ id: 'd1', name: 'Spanish', createdAt: 1, schedulerKind: 'sm2' }],
  cards: [
    {
      id: 'c1',
      deckId: 'd1',
      front: 'hola',
      back: 'hello',
      createdAt: 2,
      updatedAt: 3,
      scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 4 },
    },
  ],
  tombstones: [{ id: 'c9', kind: 'card', deletedAt: 5 }],
}

describe('snapshot', () => {
  it('round-trips a snapshot through files', () => {
    const files = serializeSnapshot(sample)
    expect(Object.keys(files)).toContain('rem.json')
    expect(Object.keys(files)).toContain('decks/d1.json')
    expect(deserializeSnapshot(files)).toEqual(sample)
  })

  it('deserializes an empty file map to the empty snapshot', () => {
    expect(deserializeSnapshot({})).toEqual(EMPTY_SNAPSHOT)
  })

  it('groups cards under their deck file', () => {
    const files = serializeSnapshot(sample)
    expect(files['decks/d1.json']).toContain('hola')
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/data/sync/snapshot.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `snapshot.ts`**

Create `src/data/sync/snapshot.ts`:

```ts
import type { SchedulerKind, SchedulingState, Tombstone } from '../../domain/models'

export type { Tombstone }

export interface DeckRecord {
  id: string
  name: string
  createdAt: number
  schedulerKind: SchedulerKind
}

export interface CardRecord {
  id: string
  deckId: string
  front: string
  back: string
  createdAt: number
  updatedAt: number
  scheduling: SchedulingState
}

export interface RepoSnapshot {
  decks: DeckRecord[]
  cards: CardRecord[]
  tombstones: Tombstone[]
}

export const SYNC_FORMAT = 'rem-sync'
export const SYNC_VERSION = 1
export const EMPTY_SNAPSHOT: RepoSnapshot = { decks: [], cards: [], tombstones: [] }

interface DeckFile {
  deck: DeckRecord
  cards: CardRecord[]
}

/** Serialize a snapshot to the file-per-deck layout: rem.json manifest,
 *  decks/<id>.json (deck + its cards), tombstones.json. */
export function serializeSnapshot(snap: RepoSnapshot): Record<string, string> {
  const files: Record<string, string> = {}
  files['rem.json'] = JSON.stringify({ format: SYNC_FORMAT, version: SYNC_VERSION }, null, 2)

  const cardsByDeck = new Map<string, CardRecord[]>()
  for (const c of snap.cards) {
    const arr = cardsByDeck.get(c.deckId) ?? []
    arr.push(c)
    cardsByDeck.set(c.deckId, arr)
  }
  for (const deck of snap.decks) {
    const payload: DeckFile = { deck, cards: cardsByDeck.get(deck.id) ?? [] }
    files[`decks/${deck.id}.json`] = JSON.stringify(payload, null, 2)
  }
  files['tombstones.json'] = JSON.stringify(snap.tombstones, null, 2)
  return files
}

/** Inverse of {@link serializeSnapshot}. Unknown paths (e.g. future reviews/)
 *  are ignored. An empty map yields the empty snapshot. */
export function deserializeSnapshot(files: Record<string, string>): RepoSnapshot {
  const decks: DeckRecord[] = []
  const cards: CardRecord[] = []
  let tombstones: Tombstone[] = []
  for (const [path, content] of Object.entries(files)) {
    if (path === 'rem.json') continue
    if (path === 'tombstones.json') {
      tombstones = JSON.parse(content) as Tombstone[]
      continue
    }
    if (path.startsWith('decks/') && path.endsWith('.json')) {
      const { deck, cards: deckCards } = JSON.parse(content) as DeckFile
      decks.push(deck)
      for (const c of deckCards) cards.push(c)
    }
  }
  return { decks, cards, tombstones }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npx vitest run src/data/sync/snapshot.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/data/sync/snapshot.ts src/data/sync/snapshot.test.ts
git commit -m "feat: rem-sync snapshot serialize/deserialize (file per deck)"
```

---

### Task 4: Pure merge (LWW + tombstones)

**Files:**
- Create: `src/data/sync/merge.ts`
- Test: `src/data/sync/merge.test.ts`

**Interfaces:**
- Consumes: `RepoSnapshot`, `DeckRecord`, `CardRecord`, `Tombstone` from `./snapshot`.
- Produces: `DbOps = { upsertDecks: DeckRecord[]; upsertCards: CardRecord[]; deleteDeckIds: string[]; deleteCardIds: string[]; tombstones: Tombstone[] }`; `MergeResult = { merged: RepoSnapshot; dbOps: DbOps }`; `merge(local, remote): MergeResult`.

- [ ] **Step 1: Write the failing tests**

Create `src/data/sync/merge.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { merge } from './merge'
import type { RepoSnapshot, CardRecord, DeckRecord } from './snapshot'

const deck: DeckRecord = { id: 'd1', name: 'D', createdAt: 1, schedulerKind: 'sm2' }
function card(id: string, updatedAt: number, front = 'f'): CardRecord {
  return {
    id, deckId: 'd1', front, back: 'b', createdAt: 1, updatedAt,
    scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 0 },
  }
}
function snap(p: Partial<RepoSnapshot>): RepoSnapshot {
  return { decks: [], cards: [], tombstones: [], ...p }
}

describe('merge', () => {
  it('unions cards edited on different sides', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10)] })
    const remote = snap({ decks: [deck], cards: [card('b', 10)] })
    const { merged } = merge(local, remote)
    expect(merged.cards.map((c) => c.id).sort()).toEqual(['a', 'b'])
  })

  it('keeps the newer edit for the same card (LWW)', () => {
    const local = snap({ decks: [deck], cards: [card('a', 20, 'local')] })
    const remote = snap({ decks: [deck], cards: [card('a', 10, 'remote')] })
    const { merged } = merge(local, remote)
    expect(merged.cards).toHaveLength(1)
    expect(merged.cards[0].front).toBe('local')
  })

  it('a tombstone newer than the edit deletes the card', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10)] })
    const remote = snap({ decks: [deck], tombstones: [{ id: 'a', kind: 'card', deletedAt: 20 }] })
    const { merged, dbOps } = merge(local, remote)
    expect(merged.cards).toHaveLength(0)
    expect(dbOps.deleteCardIds).toEqual(['a'])
  })

  it('an edit newer than the tombstone keeps the card', () => {
    const local = snap({ decks: [deck], cards: [card('a', 30)] })
    const remote = snap({ decks: [deck], tombstones: [{ id: 'a', kind: 'card', deletedAt: 20 }] })
    const { merged } = merge(local, remote)
    expect(merged.cards.map((c) => c.id)).toEqual(['a'])
  })

  it('a deck tombstone removes the deck and cascades to its cards', () => {
    const local = snap({ decks: [deck], cards: [card('a', 10)] })
    const remote = snap({ tombstones: [{ id: 'd1', kind: 'deck', deletedAt: 50 }] })
    const { merged, dbOps } = merge(local, remote)
    expect(merged.decks).toHaveLength(0)
    expect(merged.cards).toHaveLength(0)
    expect(dbOps.deleteDeckIds).toEqual(['d1'])
    expect(dbOps.deleteCardIds).toEqual(['a'])
  })

  it('pulls a remote-only deck into dbOps upserts', () => {
    const remote = snap({ decks: [deck], cards: [card('a', 10)] })
    const { dbOps } = merge(snap({}), remote)
    expect(dbOps.upsertDecks).toEqual([deck])
    expect(dbOps.upsertCards.map((c) => c.id)).toEqual(['a'])
  })

  it('keeps the newest tombstone when both sides have one', () => {
    const local = snap({ tombstones: [{ id: 'a', kind: 'card', deletedAt: 10 }] })
    const remote = snap({ tombstones: [{ id: 'a', kind: 'card', deletedAt: 99 }] })
    const { merged } = merge(local, remote)
    expect(merged.tombstones).toEqual([{ id: 'a', kind: 'card', deletedAt: 99 }])
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/data/sync/merge.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `merge.ts`**

Create `src/data/sync/merge.ts`:

```ts
import type { CardRecord, DeckRecord, RepoSnapshot, Tombstone } from './snapshot'

export interface DbOps {
  upsertDecks: DeckRecord[]
  upsertCards: CardRecord[]
  deleteDeckIds: string[]
  deleteCardIds: string[]
  tombstones: Tombstone[]
}

export interface MergeResult {
  merged: RepoSnapshot
  dbOps: DbOps
}

function newestTombstones(a: Tombstone[], b: Tombstone[]): Map<string, Tombstone> {
  const map = new Map<string, Tombstone>()
  for (const t of [...a, ...b]) {
    const prev = map.get(t.id)
    if (!prev || t.deletedAt > prev.deletedAt) map.set(t.id, t)
  }
  return map
}

/** Reconcile two snapshots: per-card last-writer-wins by updatedAt, with
 *  tombstones removing records whose deletion is newer than their last edit.
 *  `dbOps` reconciles the LOCAL store toward `merged` (upserts are idempotent). */
export function merge(local: RepoSnapshot, remote: RepoSnapshot): MergeResult {
  const tombstones = newestTombstones(local.tombstones, remote.tombstones)

  // Decks: union by id (decks are immutable today). Drop if a deck tombstone
  // is at/after the deck's creation (a re-created deck gets a fresh id, so the
  // tombstone never shadows it).
  const deckById = new Map<string, DeckRecord>()
  for (const d of [...remote.decks, ...local.decks]) deckById.set(d.id, d)
  const mergedDecks: DeckRecord[] = []
  for (const d of deckById.values()) {
    const t = tombstones.get(d.id)
    if (t && t.kind === 'deck' && t.deletedAt >= d.createdAt) continue
    mergedDecks.push(d)
  }
  const liveDeckIds = new Set(mergedDecks.map((d) => d.id))

  // Cards: union by id, newest updatedAt wins; drop if deck gone or tombstoned.
  const cardById = new Map<string, CardRecord>()
  for (const c of [...remote.cards, ...local.cards]) {
    const prev = cardById.get(c.id)
    if (!prev || c.updatedAt > prev.updatedAt) cardById.set(c.id, c)
  }
  const mergedCards: CardRecord[] = []
  for (const c of cardById.values()) {
    if (!liveDeckIds.has(c.deckId)) continue
    const t = tombstones.get(c.id)
    if (t && t.kind === 'card' && t.deletedAt > c.updatedAt) continue
    mergedCards.push(c)
  }

  const merged: RepoSnapshot = {
    decks: mergedDecks,
    cards: mergedCards,
    tombstones: [...tombstones.values()],
  }

  const mergedDeckIds = new Set(mergedDecks.map((d) => d.id))
  const mergedCardIds = new Set(mergedCards.map((c) => c.id))
  const dbOps: DbOps = {
    upsertDecks: mergedDecks,
    upsertCards: mergedCards,
    deleteDeckIds: local.decks.filter((d) => !mergedDeckIds.has(d.id)).map((d) => d.id),
    deleteCardIds: local.cards.filter((c) => !mergedCardIds.has(c.id)).map((c) => c.id),
    tombstones: merged.tombstones,
  }

  return { merged, dbOps }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/data/sync/merge.test.ts`
Expected: PASS (7 tests)

- [ ] **Step 5: Commit**

```bash
git add src/data/sync/merge.ts src/data/sync/merge.test.ts
git commit -m "feat: pure LWW merge with tombstones"
```

---

### Task 5: Storage `exportSnapshot` / `applyMerge` + tombstones on delete

**Files:**
- Modify: `src/data/Storage.ts`
- Modify: `src/data/dexie/DexieStorage.ts`
- Test: `src/data/dexie/DexieStorage.test.ts` (add a `describe`)

**Interfaces:**
- Consumes: `RepoSnapshot` from `../sync/snapshot`; `DbOps` from `../sync/merge`.
- Produces: `Storage.exportSnapshot(): Promise<RepoSnapshot>`; `Storage.applyMerge(ops: DbOps): Promise<void>`; `deleteDeck`/`deleteCard` now write a `Tombstone`.

- [ ] **Step 1: Write the failing tests**

Append to `src/data/dexie/DexieStorage.test.ts`:

```ts
describe('sync storage', () => {
  it('exportSnapshot returns decks, cards, and tombstones', async () => {
    const deck = await storage.createDeck('S')
    await storage.createCard(deck.id, 'q', 'a')
    const snap = await storage.exportSnapshot()
    expect(snap.decks).toHaveLength(1)
    expect(snap.cards).toHaveLength(1)
    expect(snap.tombstones).toHaveLength(0)
  })

  it('deleteCard writes a card tombstone', async () => {
    const deck = await storage.createDeck('S')
    const c = await storage.createCard(deck.id, 'q', 'a')
    await storage.deleteCard(c.id)
    const snap = await storage.exportSnapshot()
    expect(snap.tombstones).toEqual([
      expect.objectContaining({ id: c.id, kind: 'card' }),
    ])
  })

  it('deleteDeck writes a deck tombstone', async () => {
    const deck = await storage.createDeck('S')
    await storage.deleteDeck(deck.id)
    const snap = await storage.exportSnapshot()
    expect(snap.tombstones).toEqual([
      expect.objectContaining({ id: deck.id, kind: 'deck' }),
    ])
  })

  it('applyMerge upserts and deletes records', async () => {
    const deck = await storage.createDeck('S')
    const stale = await storage.createCard(deck.id, 'old', 'old')
    await storage.applyMerge({
      upsertDecks: [{ id: deck.id, name: 'S', createdAt: deck.createdAt, schedulerKind: 'sm2' }],
      upsertCards: [{
        id: 'new', deckId: deck.id, front: 'new', back: 'new', createdAt: 1, updatedAt: 2,
        scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 0 },
      }],
      deleteDeckIds: [],
      deleteCardIds: [stale.id],
      tombstones: [{ id: stale.id, kind: 'card', deletedAt: 5 }],
    })
    expect(await storage.getCard(stale.id)).toBeUndefined()
    expect(await storage.getCard('new')).toBeTruthy()
    expect((await storage.exportSnapshot()).tombstones).toHaveLength(1)
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — `exportSnapshot`/`applyMerge` not on storage.

- [ ] **Step 3: Extend the `Storage` interface**

In `src/data/Storage.ts`, add imports and methods:

```ts
import type { RepoSnapshot } from './sync/snapshot'
import type { DbOps } from './sync/merge'
```

Add to the `Storage` interface (after `importDecks`):

```ts
  /** Full point-in-time snapshot of the store, for sync. */
  exportSnapshot(): Promise<RepoSnapshot>
  /** Apply a merge result: upsert records, delete by id, persist tombstones. */
  applyMerge(ops: DbOps): Promise<void>
```

- [ ] **Step 4: Implement in `DexieStorage`**

In `src/data/dexie/DexieStorage.ts`:

Add imports at the top:

```ts
import type { RepoSnapshot } from '../sync/snapshot'
import type { DbOps } from '../sync/merge'
```

Replace `deleteDeck` and `deleteCard` with tombstone-writing versions:

```ts
  async deleteDeck(id: ID): Promise<void> {
    await this.db.transaction('rw', this.db.decks, this.db.cards, this.db.tombstones, async () => {
      await this.db.cards.where('deckId').equals(id).delete()
      await this.db.decks.delete(id)
      await this.db.tombstones.put({ id, kind: 'deck', deletedAt: Date.now() })
    })
  }
```

```ts
  async deleteCard(id: ID): Promise<void> {
    await this.db.transaction('rw', this.db.cards, this.db.tombstones, async () => {
      await this.db.cards.delete(id)
      await this.db.tombstones.put({ id, kind: 'card', deletedAt: Date.now() })
    })
  }
```

Add the two new methods (e.g. after `importDecks`):

```ts
  async exportSnapshot(): Promise<RepoSnapshot> {
    const [decks, cards, tombstones] = await Promise.all([
      this.db.decks.toArray(),
      this.db.cards.toArray(),
      this.db.tombstones.toArray(),
    ])
    return { decks, cards, tombstones }
  }

  async applyMerge(ops: DbOps): Promise<void> {
    await this.db.transaction('rw', this.db.decks, this.db.cards, this.db.tombstones, async () => {
      if (ops.deleteCardIds.length) await this.db.cards.bulkDelete(ops.deleteCardIds)
      if (ops.deleteDeckIds.length) await this.db.decks.bulkDelete(ops.deleteDeckIds)
      if (ops.upsertDecks.length) await this.db.decks.bulkPut(ops.upsertDecks)
      if (ops.upsertCards.length) await this.db.cards.bulkPut(ops.upsertCards)
      if (ops.tombstones.length) await this.db.tombstones.bulkPut(ops.tombstones)
    })
  }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: PASS (existing + 4 new)

- [ ] **Step 6: Typecheck and commit**

Run: `npm run typecheck`
Expected: no errors.

```bash
git add src/data/Storage.ts src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat: Storage exportSnapshot/applyMerge + tombstones on delete"
```

---

## Phase 2 — Transport + service

### Task 6: `GitBridge` interface + `FakeGitBridge`

**Files:**
- Create: `src/data/sync/GitBridge.ts`
- Create: `src/data/sync/FakeGitBridge.ts`
- Test: `src/data/sync/FakeGitBridge.test.ts`

**Interfaces:**
- Produces: `GitBridge` interface; `CommitPushResult = { pushed: boolean; rejected: boolean }`; `FetchResetResult = { remoteExists: boolean }`; `FakeGitBridge` with public `remote: Record<string,string> | null` and `pushInterceptor: (() => void) | null`.

- [ ] **Step 1: Write the failing test**

Create `src/data/sync/FakeGitBridge.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { FakeGitBridge } from './FakeGitBridge'

describe('FakeGitBridge', () => {
  it('reports an empty remote until first push', async () => {
    const b = new FakeGitBridge(null)
    await b.clone('url', 'dir')
    const { remoteExists } = await b.fetchReset('dir')
    expect(remoteExists).toBe(false)
    await b.writeFiles('dir', { 'rem.json': '{}' })
    const res = await b.commitPush('dir', 'msg')
    expect(res).toEqual({ pushed: true, rejected: false })
    expect(b.remote).toEqual({ 'rem.json': '{}' })
  })

  it('rejects a push when the remote advanced mid-sync, then accepts on retry', async () => {
    const b = new FakeGitBridge({ 'rem.json': '{}' })
    await b.clone('url', 'dir')
    await b.fetchReset('dir')
    b.pushInterceptor = () => { b.remote = { 'rem.json': '{}', 'x': '1' }; b.bumpRemote() }
    await b.writeFiles('dir', { 'rem.json': '{}', 'mine': '2' })
    expect(await b.commitPush('dir', 'm')).toEqual({ pushed: false, rejected: true })
    await b.fetchReset('dir')
    await b.writeFiles('dir', { 'rem.json': '{}', 'merged': '3' })
    expect(await b.commitPush('dir', 'm')).toEqual({ pushed: true, rejected: false })
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/data/sync/FakeGitBridge.test.ts`
Expected: FAIL — modules not found.

- [ ] **Step 3: Implement `GitBridge.ts`**

Create `src/data/sync/GitBridge.ts`:

```ts
export interface CommitPushResult {
  pushed: boolean
  /** True when the push was a non-fast-forward (remote advanced); the caller retries. */
  rejected: boolean
}

export interface FetchResetResult {
  /** False when the remote has no `main` branch yet (fresh/empty repo). */
  remoteExists: boolean
}

/** Dumb git transport. Implementations: {@link ./TauriGitBridge} (real) and
 *  {@link ./FakeGitBridge} (tests). All paths are absolute working-copy dirs. */
export interface GitBridge {
  isCloned(dir: string): Promise<boolean>
  clone(remoteUrl: string, dir: string): Promise<void>
  fetchReset(dir: string): Promise<FetchResetResult>
  readFiles(dir: string): Promise<Record<string, string>>
  writeFiles(dir: string, files: Record<string, string>): Promise<void>
  commitPush(dir: string, message: string): Promise<CommitPushResult>
}
```

- [ ] **Step 4: Implement `FakeGitBridge.ts`**

Create `src/data/sync/FakeGitBridge.ts`:

```ts
import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'

/** In-memory GitBridge for tests. `remote === null` models an empty remote
 *  (no `main` yet). `pushInterceptor` fires once at the next commitPush to
 *  simulate a concurrent push (forcing a rejection). */
export class FakeGitBridge implements GitBridge {
  remote: Record<string, string> | null
  pushInterceptor: (() => void) | null = null
  private working: Record<string, string> = {}
  private cloned = false
  private remoteVersion = 0
  private fetchedVersion = -1

  constructor(remote: Record<string, string> | null = null) {
    this.remote = remote
    if (remote) this.remoteVersion = 1
  }

  /** Test helper: mark the remote as advanced (used inside pushInterceptor). */
  bumpRemote(): void {
    this.remoteVersion++
  }

  async isCloned(): Promise<boolean> {
    return this.cloned
  }

  async clone(): Promise<void> {
    this.cloned = true
    this.working = this.remote ? { ...this.remote } : {}
  }

  async fetchReset(): Promise<FetchResetResult> {
    this.working = this.remote ? { ...this.remote } : {}
    this.fetchedVersion = this.remoteVersion
    return { remoteExists: this.remote !== null }
  }

  async readFiles(): Promise<Record<string, string>> {
    return { ...this.working }
  }

  async writeFiles(_dir: string, files: Record<string, string>): Promise<void> {
    this.working = { ...files }
  }

  async commitPush(): Promise<CommitPushResult> {
    if (this.pushInterceptor) {
      const fn = this.pushInterceptor
      this.pushInterceptor = null
      fn()
    }
    if (this.remoteVersion !== this.fetchedVersion) {
      return { pushed: false, rejected: true }
    }
    this.remote = { ...this.working }
    this.remoteVersion++
    this.fetchedVersion = this.remoteVersion
    return { pushed: true, rejected: false }
  }
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `npx vitest run src/data/sync/FakeGitBridge.test.ts`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/data/sync/GitBridge.ts src/data/sync/FakeGitBridge.ts src/data/sync/FakeGitBridge.test.ts
git commit -m "feat: GitBridge interface + in-memory FakeGitBridge"
```

---

### Task 7: `GitSyncService` (the protocol)

**Files:**
- Create: `src/data/sync/GitSyncService.ts`
- Test: `src/data/sync/GitSyncService.test.ts`

**Interfaces:**
- Consumes: `Storage`; `GitBridge`; `merge`; `serializeSnapshot`/`deserializeSnapshot`/`EMPTY_SNAPSHOT`.
- Produces: `SyncConfig = { remoteUrl: string; repoDir: string }`; `SyncOutcome = { pushed: boolean }`; `GitSyncService` with `sync(): Promise<SyncOutcome>`.

- [ ] **Step 1: Write the failing tests**

Create `src/data/sync/GitSyncService.test.ts`:

```ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../dexie/db'
import { DexieStorage } from '../dexie/DexieStorage'
import { FakeGitBridge } from './FakeGitBridge'
import { GitSyncService } from './GitSyncService'
import { serializeSnapshot } from './snapshot'

const DB = 'rem-sync-service-test'
let db: RemDB
let storage: DexieStorage

beforeEach(async () => {
  await Dexie.delete(DB)
  db = new RemDB(DB)
  storage = new DexieStorage(db)
})
afterEach(() => db.close())

const cfg = { remoteUrl: 'url', repoDir: 'dir' }

describe('GitSyncService', () => {
  it('pushes local data to an empty remote on first sync', async () => {
    const deck = await storage.createDeck('S')
    await storage.createCard(deck.id, 'q', 'a')
    const bridge = new FakeGitBridge(null)
    await new GitSyncService(storage, bridge, cfg).sync()
    expect(bridge.remote).not.toBeNull()
    expect(Object.keys(bridge.remote!)).toContain(`decks/${deck.id}.json`)
  })

  it('pulls a remote-only deck into the local store', async () => {
    const remote = serializeSnapshot({
      decks: [{ id: 'd1', name: 'Remote', createdAt: 1, schedulerKind: 'sm2' }],
      cards: [],
      tombstones: [],
    })
    const bridge = new FakeGitBridge(remote)
    await new GitSyncService(storage, bridge, cfg).sync()
    const decks = await storage.listDecks()
    expect(decks.map((d) => d.name)).toEqual(['Remote'])
  })

  it('applies a remote tombstone to delete a local card', async () => {
    const deck = await storage.createDeck('S')
    const c = await storage.createCard(deck.id, 'q', 'a')
    const remote = serializeSnapshot({
      decks: [{ id: deck.id, name: 'S', createdAt: deck.createdAt, schedulerKind: 'sm2' }],
      cards: [],
      tombstones: [{ id: c.id, kind: 'card', deletedAt: Date.now() + 10000 }],
    })
    const bridge = new FakeGitBridge(remote)
    await new GitSyncService(storage, bridge, cfg).sync()
    expect(await storage.getCard(c.id)).toBeUndefined()
  })

  it('retries when the push is rejected, then succeeds', async () => {
    await storage.createDeck('S')
    const bridge = new FakeGitBridge({ 'rem.json': '{}' })
    bridge.pushInterceptor = () => {
      bridge.remote = { 'rem.json': '{}', 'tombstones.json': '[]' }
      bridge.bumpRemote()
    }
    const outcome = await new GitSyncService(storage, bridge, cfg).sync()
    expect(outcome.pushed).toBe(true)
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/data/sync/GitSyncService.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `GitSyncService.ts`**

Create `src/data/sync/GitSyncService.ts`:

```ts
import type { Storage } from '../Storage'
import type { GitBridge } from './GitBridge'
import { merge } from './merge'
import { deserializeSnapshot, serializeSnapshot, EMPTY_SNAPSHOT } from './snapshot'

export interface SyncConfig {
  remoteUrl: string
  repoDir: string
}

export interface SyncOutcome {
  pushed: boolean
}

const MAX_PUSH_ATTEMPTS = 5

/** Orchestrates the sync protocol: reset working copy to remote, merge in-app,
 *  write back, commit, push — retrying if the remote advanced mid-sync. */
export class GitSyncService {
  constructor(
    private readonly storage: Storage,
    private readonly bridge: GitBridge,
    private readonly config: SyncConfig,
  ) {}

  async sync(): Promise<SyncOutcome> {
    const { remoteUrl, repoDir } = this.config
    if (!(await this.bridge.isCloned(repoDir))) {
      await this.bridge.clone(remoteUrl, repoDir)
    }
    for (let attempt = 0; attempt < MAX_PUSH_ATTEMPTS; attempt++) {
      const { remoteExists } = await this.bridge.fetchReset(repoDir)
      const remote = remoteExists
        ? deserializeSnapshot(await this.bridge.readFiles(repoDir))
        : EMPTY_SNAPSHOT
      const local = await this.storage.exportSnapshot()
      const { merged, dbOps } = merge(local, remote)
      await this.storage.applyMerge(dbOps)
      await this.bridge.writeFiles(repoDir, serializeSnapshot(merged))
      const { pushed, rejected } = await this.bridge.commitPush(
        repoDir,
        `sync ${new Date().toISOString()}`,
      )
      if (!rejected) return { pushed }
    }
    throw new Error('Sync failed: the remote kept changing during push. Try again.')
  }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run src/data/sync/GitSyncService.test.ts`
Expected: PASS (4 tests)

- [ ] **Step 5: Commit**

```bash
git add src/data/sync/GitSyncService.ts src/data/sync/GitSyncService.test.ts
git commit -m "feat: GitSyncService 8-step protocol with push-retry"
```

---

### Task 8: Rust git commands

**Files:**
- Create: `src-tauri/src/git.rs`
- Modify: `src-tauri/src/lib.rs` (register commands)

**Interfaces:**
- Produces Tauri commands (invoked from TS in Task 9): `git_is_cloned(dir) -> bool`, `git_clone(remote_url, dir)`, `git_fetch_reset(dir) -> bool`, `git_read_files(dir) -> HashMap<String,String>`, `git_write_files(dir, files: HashMap<String,String>)`, `git_commit_push(dir, message) -> CommitPushResult { pushed, rejected }`.

- [ ] **Step 1: Write `src-tauri/src/git.rs`**

```rust
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Serialize)]
pub struct CommitPushResult {
    pushed: bool,
    rejected: bool,
}

/// Run git in `dir`, returning (stdout, stderr, success). Maps a missing git
/// binary to a recognizable error string.
fn run_git(args: &[&str], dir: &str) -> Result<(String, String, bool), String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "git-not-installed".to_string()
            } else {
                format!("failed to run git: {e}")
            }
        })?;
    Ok((
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    ))
}

fn ok_or_stderr(res: (String, String, bool)) -> Result<String, String> {
    let (stdout, stderr, success) = res;
    if success { Ok(stdout) } else { Err(stderr) }
}

#[tauri::command]
pub fn git_is_cloned(dir: String) -> Result<bool, String> {
    Ok(Path::new(&dir).join(".git").exists())
}

#[tauri::command]
pub fn git_clone(remote_url: String, dir: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&dir).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Clone into `dir` (its parent must exist; `dir` itself is created by git).
    let parent = Path::new(&dir)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    let name = Path::new(&dir)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or("invalid repo dir")?;
    ok_or_stderr(run_git(&["clone", &remote_url, &name], &parent))?;
    Ok(())
}

#[tauri::command]
pub fn git_fetch_reset(dir: String) -> Result<bool, String> {
    ok_or_stderr(run_git(&["fetch", "origin"], &dir))?;
    let (_, _, has_main) = run_git(&["rev-parse", "--verify", "origin/main"], &dir)?;
    if !has_main {
        return Ok(false); // empty remote: no main branch yet
    }
    ok_or_stderr(run_git(&["reset", "--hard", "origin/main"], &dir))?;
    Ok(true)
}

/// Recursively collect tracked content files (decks/, rem.json, tombstones.json)
/// as a path->content map, with forward-slash relative paths.
fn collect_files(root: &Path, dir: &Path, out: &mut HashMap<String, String>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            out.insert(rel, content);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn git_read_files(dir: String) -> Result<HashMap<String, String>, String> {
    let root = PathBuf::from(&dir);
    let mut out = HashMap::new();
    collect_files(&root, &root, &mut out)?;
    Ok(out)
}

#[tauri::command]
pub fn git_write_files(dir: String, files: HashMap<String, String>) -> Result<(), String> {
    let root = PathBuf::from(&dir);
    // Clear the managed set so deletions take effect, then write incoming files.
    let _ = fs::remove_dir_all(root.join("decks"));
    let _ = fs::remove_file(root.join("tombstones.json"));
    let _ = fs::remove_file(root.join("rem.json"));
    for (rel, content) in files {
        let full = root.join(&rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&full, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn git_commit_push(dir: String, message: String) -> Result<CommitPushResult, String> {
    ok_or_stderr(run_git(&["add", "-A"], &dir))?;
    // Commit only if something is staged. Identity passed inline so commits work
    // even without global git config.
    let (_, _, has_changes) = run_git(&["diff", "--cached", "--quiet"], &dir)?;
    if !has_changes {
        ok_or_stderr(run_git(
            &[
                "-c", "user.name=rem", "-c", "user.email=rem@localhost",
                "commit", "-m", &message,
            ],
            &dir,
        ))?;
    }
    // Push to main; classify non-fast-forward as a (retryable) rejection.
    let (_, stderr, success) = run_git(&["push", "origin", "HEAD:main"], &dir)?;
    if success {
        return Ok(CommitPushResult { pushed: true, rejected: false });
    }
    let lower = stderr.to_lowercase();
    if lower.contains("rejected") || lower.contains("non-fast-forward") || lower.contains("fetch first") {
        return Ok(CommitPushResult { pushed: false, rejected: true });
    }
    Err(stderr)
}
```

> Note on `git diff --cached --quiet`: exit code 0 means *no* staged changes, 1 means there *are* changes. `run_git` returns `success = true` for exit 0, so `has_changes` is `true` when there are **no** changes — the variable name reads inverted on purpose; commit runs in the `if !has_changes` branch (i.e., when changes exist). Keep this exactly as written.

- [ ] **Step 2: Register the commands in `lib.rs`**

Open the generated `src-tauri/src/lib.rs`. It contains a sample `greet` command and a `run()` function with `invoke_handler(tauri::generate_handler![greet])`. Replace the command list and add the module. The file should read:

```rust
mod git;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            git::git_is_cloned,
            git::git_clone,
            git::git_fetch_reset,
            git::git_read_files,
            git::git_write_files,
            git::git_commit_push,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

(Delete the sample `greet` function. Keep the `tauri_plugin_opener` line only if the generated file has it; otherwise omit.)

- [ ] **Step 3: Add `serde` to Cargo if missing**

Ensure `src-tauri/Cargo.toml` `[dependencies]` includes serde (Tauri pulls it in transitively, but make it explicit):

```toml
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors. (Returns to repo root after.)

- [ ] **Step 5: Manually verify the round trip against a scratch repo**

Create a throwaway bare repo and exercise the commands via the desktop app's devtools console (run `npm run app:dev`, open devtools, and use `window.__TAURI__`), OR trust the Task 12 end-to-end. At minimum confirm `cargo check` passes and the app still launches (`npm run app:dev`).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/git.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: Rust git commands (clone/fetch-reset/read/write/commit-push)"
```

---

### Task 9: `TauriGitBridge` (TS → Tauri commands)

**Files:**
- Create: `src/data/sync/TauriGitBridge.ts`

**Interfaces:**
- Consumes: `GitBridge`, `CommitPushResult`, `FetchResetResult`; `invoke` from `@tauri-apps/api/core`.
- Produces: `TauriGitBridge implements GitBridge`. (Tauri v2 maps JS camelCase args, e.g. `remoteUrl`, to Rust `remote_url`.)

- [ ] **Step 1: Implement the bridge**

Create `src/data/sync/TauriGitBridge.ts`:

```ts
import { invoke } from '@tauri-apps/api/core'
import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'

/** Real GitBridge: forwards each call to the Rust commands from Task 8. */
export class TauriGitBridge implements GitBridge {
  isCloned(dir: string): Promise<boolean> {
    return invoke<boolean>('git_is_cloned', { dir })
  }

  clone(remoteUrl: string, dir: string): Promise<void> {
    return invoke<void>('git_clone', { remoteUrl, dir })
  }

  async fetchReset(dir: string): Promise<FetchResetResult> {
    const remoteExists = await invoke<boolean>('git_fetch_reset', { dir })
    return { remoteExists }
  }

  readFiles(dir: string): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('git_read_files', { dir })
  }

  writeFiles(dir: string, files: Record<string, string>): Promise<void> {
    return invoke<void>('git_write_files', { dir, files })
  }

  commitPush(dir: string, message: string): Promise<CommitPushResult> {
    return invoke<CommitPushResult>('git_commit_push', { dir, message })
  }
}
```

- [ ] **Step 2: Typecheck**

Run: `npm run typecheck`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/data/sync/TauriGitBridge.ts
git commit -m "feat: TauriGitBridge wiring TS to Rust git commands"
```

---

## Phase 3 — UI + wiring

### Task 10: Settings Sync section

**Files:**
- Create: `src/features/settings/SyncSection.tsx`
- Create: `src/features/settings/SyncSection.browser.test.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`

**Interfaces:**
- Consumes: `useStorage`; `GitSyncService`, `SyncConfig`; `TauriGitBridge`; `isTauri` from `@tauri-apps/api`.
- Produces: `SyncSection` component. Props: `{ makeService?: (storage: Storage, cfg: SyncConfig) => GitSyncService }` (tests inject a fake-backed service). Reads/writes localStorage key `rem.sync.remoteUrl`; writes `rem.sync.lastSyncAt`.

- [ ] **Step 1: Write the failing browser test**

Create `src/features/settings/SyncSection.browser.test.tsx`:

```tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from '@testing-library/user-event'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { FakeGitBridge } from '../../data/sync/FakeGitBridge'
import { GitSyncService } from '../../data/sync/GitSyncService'
import { SyncSection } from './SyncSection'

beforeEach(() => localStorage.clear())

describe('SyncSection', () => {
  it('syncs on click and reports success', async () => {
    const storage = new DexieStorage(new RemDB('sync-ui-test'))
    await storage.createDeck('S')
    const bridge = new FakeGitBridge(null)
    const makeService = (s: any, cfg: any) => new GitSyncService(s, bridge, cfg)

    const screen = render(
      <StorageProvider storage={storage}>
        <SyncSection makeService={makeService} />
      </StorageProvider>,
    )

    await userEvent.type(screen.getByLabelText('Git remote URL'), 'git@example.com:me/rem.git')
    await userEvent.click(screen.getByRole('button', { name: 'Sync now' }))

    await expect.element(screen.getByText(/synced/i)).toBeInTheDocument()
    expect(bridge.remote).not.toBeNull()
  })

  it('refuses to sync without a remote URL', async () => {
    const storage = new DexieStorage(new RemDB('sync-ui-test-2'))
    const makeService = (s: any, cfg: any) =>
      new GitSyncService(s, new FakeGitBridge(null), cfg)
    const screen = render(
      <StorageProvider storage={storage}>
        <SyncSection makeService={makeService} />
      </StorageProvider>,
    )
    await userEvent.click(screen.getByRole('button', { name: 'Sync now' }))
    await expect.element(screen.getByText(/enter a git remote url/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/features/settings/SyncSection.browser.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `SyncSection.tsx`**

Create `src/features/settings/SyncSection.tsx`:

```tsx
import { useState } from 'react'
import { isTauri } from '@tauri-apps/api'
import { appDataDir, join } from '@tauri-apps/api/path'
import type { Storage } from '../../data/Storage'
import { useStorage } from '../../data/StorageContext'
import { GitSyncService, type SyncConfig } from '../../data/sync/GitSyncService'
import { TauriGitBridge } from '../../data/sync/TauriGitBridge'

const REMOTE_KEY = 'rem.sync.remoteUrl'
const LAST_SYNC_KEY = 'rem.sync.lastSyncAt'

type Status =
  | { kind: 'idle' }
  | { kind: 'syncing' }
  | { kind: 'ok'; at: number }
  | { kind: 'error'; message: string }

function defaultMakeService(storage: Storage, cfg: SyncConfig): GitSyncService {
  return new GitSyncService(storage, new TauriGitBridge(), cfg)
}

async function resolveRepoDir(): Promise<string> {
  if (!isTauri()) return 'repo'
  return join(await appDataDir(), 'repo')
}

export function SyncSection({
  makeService = defaultMakeService,
}: {
  makeService?: (storage: Storage, cfg: SyncConfig) => GitSyncService
}) {
  const storage = useStorage()
  const [remoteUrl, setRemoteUrl] = useState(() => localStorage.getItem(REMOTE_KEY) ?? '')
  const lastSyncAt = Number(localStorage.getItem(LAST_SYNC_KEY)) || 0
  const [status, setStatus] = useState<Status>(
    lastSyncAt ? { kind: 'ok', at: lastSyncAt } : { kind: 'idle' },
  )

  function onUrlChange(value: string) {
    setRemoteUrl(value)
    localStorage.setItem(REMOTE_KEY, value)
  }

  async function onSync() {
    const url = remoteUrl.trim()
    if (!url) {
      setStatus({ kind: 'error', message: 'Enter a Git remote URL first.' })
      return
    }
    setStatus({ kind: 'syncing' })
    try {
      const repoDir = await resolveRepoDir()
      await makeService(storage, { remoteUrl: url, repoDir }).sync()
      const at = Date.now()
      localStorage.setItem(LAST_SYNC_KEY, String(at))
      setStatus({ kind: 'ok', at })
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Sync failed.'
      setStatus({
        kind: 'error',
        message: message === 'git-not-installed' ? 'Git is not installed on this machine.' : message,
      })
    }
  }

  return (
    <section className="settings-section">
      <h2>Sync (Git)</h2>
      <p className="settings-hint">
        Sync decks across machines via a Git remote, using your existing git credentials.
      </p>
      <label className="settings-field">
        Git remote URL
        <input
          type="text"
          aria-label="Git remote URL"
          placeholder="git@github.com:you/rem-data.git"
          value={remoteUrl}
          onChange={(e) => onUrlChange(e.target.value)}
        />
      </label>
      <button
        className="btn btn-primary"
        type="button"
        disabled={status.kind === 'syncing'}
        onClick={onSync}
      >
        {status.kind === 'syncing' ? 'Syncing…' : 'Sync now'}
      </button>
      {status.kind === 'ok' && (
        <p className="settings-ok">Synced at {new Date(status.at).toLocaleString()}.</p>
      )}
      {status.kind === 'error' && <p className="settings-error">{status.message}</p>}
    </section>
  )
}
```

- [ ] **Step 4: Render it in `SettingsPage`**

In `src/features/settings/SettingsPage.tsx`, add the import and render `<SyncSection />` as the first section inside the `<div className="stack">`:

```tsx
import { SyncSection } from './SyncSection'
```

```tsx
    <div className="stack">
      <h1 className="page-title">Settings</h1>

      <SyncSection />

      <section className="settings-section">
        <h2>Export decks</h2>
```

- [ ] **Step 5: Add the `settings-field` style**

Append to `src/ui/styles.css` (only if `.settings-field` is not already defined):

```css
.settings-field {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  max-width: 28rem;
}
.settings-field input {
  width: 100%;
}
```

- [ ] **Step 6: Run the browser test to verify it passes**

Run: `npx vitest run src/features/settings/SyncSection.browser.test.tsx`
Expected: PASS (2 tests)

- [ ] **Step 7: Commit**

```bash
git add src/features/settings/SyncSection.tsx src/features/settings/SyncSection.browser.test.tsx src/features/settings/SettingsPage.tsx src/ui/styles.css
git commit -m "feat: Settings sync section (remote URL + Sync now)"
```

---

### Task 11: Auto-sync on launch and on hide

**Files:**
- Create: `src/app/useAutoSync.ts`
- Modify: `src/app/main.tsx`

**Interfaces:**
- Consumes: `isTauri`; `resolveRepoDir` pattern; `GitSyncService` + `TauriGitBridge`; the app-wide `Storage` (the default instance).

> **Deviation from the spec, with rationale:** the spec said "auto on launch *and quit*." A webview cannot reliably *await* an async push during `beforeunload`, so a literal on-quit push would be unreliable. We sync **on launch** and **when the window is hidden** (`visibilitychange`), which fires when you switch away or minimize — strictly more reliable than on-quit. The manual "Sync now" button remains the guaranteed path.

- [ ] **Step 1: Implement the hook**

Create `src/app/useAutoSync.ts`:

```ts
import { useEffect } from 'react'
import { isTauri } from '@tauri-apps/api'
import { appDataDir, join } from '@tauri-apps/api/path'
import type { Storage } from '../data/Storage'
import { GitSyncService } from '../data/sync/GitSyncService'
import { TauriGitBridge } from '../data/sync/TauriGitBridge'

const REMOTE_KEY = 'rem.sync.remoteUrl'
const LAST_SYNC_KEY = 'rem.sync.lastSyncAt'

async function runSync(storage: Storage): Promise<void> {
  const remoteUrl = (localStorage.getItem(REMOTE_KEY) ?? '').trim()
  if (!remoteUrl) return
  const repoDir = await join(await appDataDir(), 'repo')
  await new GitSyncService(storage, new TauriGitBridge(), { remoteUrl, repoDir }).sync()
  localStorage.setItem(LAST_SYNC_KEY, String(Date.now()))
}

/** Desktop-only: sync once on launch and whenever the window is hidden.
 *  No-op in the browser build or when no remote is configured. */
export function useAutoSync(storage: Storage): void {
  useEffect(() => {
    if (!isTauri()) return
    void runSync(storage).catch(() => {})
    const onHide = () => {
      if (document.visibilityState === 'hidden') void runSync(storage).catch(() => {})
    }
    document.addEventListener('visibilitychange', onHide)
    return () => document.removeEventListener('visibilitychange', onHide)
  }, [storage])
}
```

- [ ] **Step 2: Wire it into the app entry**

The default storage instance lives in `StorageContext`. Export it so the entry can pass it to the hook. In `src/data/StorageContext.tsx`, change:

```ts
const defaultStorage: Storage = new DexieStorage(new RemDB())
```

to:

```ts
export const defaultStorage: Storage = new DexieStorage(new RemDB())
```

Then in `src/app/main.tsx`, wrap the tree in a small component that calls the hook:

```tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import { router } from './routes'
import { StorageProvider, defaultStorage } from '../data/StorageContext'
import { useAutoSync } from './useAutoSync'
import '../ui/styles.css'

function App() {
  useAutoSync(defaultStorage)
  return <RouterProvider router={router} />
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <StorageProvider>
      <App />
    </StorageProvider>
  </StrictMode>,
)
```

- [ ] **Step 3: Typecheck and run the full suite**

Run: `npm run typecheck && npm test`
Expected: typecheck clean; all tests pass (web build unaffected — `useAutoSync` is a no-op outside Tauri).

- [ ] **Step 4: Commit**

```bash
git add src/app/useAutoSync.ts src/app/main.tsx src/data/StorageContext.tsx
git commit -m "feat: auto-sync on launch and window hide (desktop only)"
```

---

### Task 12: End-to-end verification across two clones

**Files:** none (manual verification).

- [ ] **Step 1: Create a private GitHub repo** for sync data (empty, no README), e.g. `rem-data`. Confirm `git clone <url>` works from your terminal (validates your existing credentials).

- [ ] **Step 2: First machine — push.** Run `npm run app:dev`, open Settings → Sync, paste the remote URL, click **Sync now**. Expect "Synced at …". Verify on GitHub that `rem.json`, `decks/<id>.json`, and `tombstones.json` now exist.

- [ ] **Step 3: Simulate a second instance.** Quit the app. Delete the local IndexedDB (devtools → Application → IndexedDB → delete `rem`) **and** the clone dir (`rm -rf "$(node -e "...appDataDir...")/repo"`), or run on a second machine. Relaunch, set the same remote URL, click **Sync now**. Expect your decks/cards to reappear (pulled from the remote).

- [ ] **Step 4: Conflict check.** On instance A, edit a card's front; on instance B (before syncing A), edit a *different* card. Sync A, then sync B. Confirm both edits survive after a final sync on each.

- [ ] **Step 5: Deletion check.** Delete a card on A, sync. Sync B. Confirm the card disappears on B and does not resurrect on a subsequent A sync.

- [ ] **Step 6: Update the roadmap.** In `docs/ROADMAP.md`, mark the sync backend item shipped (one line, matching the existing ✅ style). Commit:

```bash
git add docs/ROADMAP.md
git commit -m "docs: mark git-backed sync shipped"
```

---

## Self-Review Notes

- **Spec coverage:** Tauri shell (Task 1) ✓; per-record LWW merge (Task 4) ✓; file-per-deck format (Task 3) ✓; tombstones (Tasks 2, 5) ✓; rem-owns-clone protocol (Tasks 7–9) ✓; auto on launch + manual button (Tasks 10–11) ✓; error handling — git-not-installed (Tasks 8, 10), offline/auth via stderr propagation (Tasks 8→10), empty remote (Tasks 7–8), push race (Tasks 6–8) ✓.
- **Deviation:** on-quit auto-sync → on-hide auto-sync, with rationale (Task 11). Flag for reviewer.
- **Out of scope (per spec):** OAuth/token storage, web sync backend, review-log/stats sync, conflict-resolution UI, tombstone GC, code-signing/auto-update. Not in any task — intentional.
- **Type consistency:** `RepoSnapshot`/`DeckRecord`/`CardRecord`/`Tombstone` (Task 3) are reused verbatim in Tasks 4, 5, 7. `DbOps` (Task 4) matches `applyMerge` (Task 5) and `GitSyncService` (Task 7). `GitBridge` method set (Task 6) matches `FakeGitBridge` (Task 6), `TauriGitBridge` (Task 9), and the Rust commands (Task 8).
