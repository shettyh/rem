# Card Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the card-editor modal with a full-screen route carrying a persistent markdown toolbar and local image/GIF support, stored as content-addressed assets behind the Storage port and synced across machines.

**Architecture:** Images become SHA-256-addressed `Asset` records in a new Dexie table, referenced from card markdown as `asset:<hash>`. A shared resolver turns those refs into object URLs for the TipTap editor and the react-markdown review view. Sync carries asset bytes as real binary files in `assets/<hash>.<ext>` via two new Rust commands and `GitBridge` methods; reconciliation is union-by-hash pruned to referenced hashes (assets are immutable, so no last-writer-wins).

**Tech Stack:** React 19 + TypeScript, TipTap v3 (`@tiptap/react`, `@tiptap/extension-image`), Dexie 4 (IndexedDB), react-markdown, Tauri v2 (Rust git bridge), Vitest (unit jsdom + browser chromium), cargo test.

## Global Constraints

- **Asset id = SHA-256 hex of the bytes**, computed with Web Crypto `crypto.subtle.digest('SHA-256', …)`. Same bytes → same id (dedupe).
- **Markdown reference syntax is exactly `asset:<hash>`** inside standard image markdown: `![alt](asset:<hash>)`. `<hash>` is 64 lowercase hex chars.
- **Asset bytes are stored as `Uint8Array`** (structured-clone-safe in IndexedDB); object URLs are built on demand via `new Blob([bytes], { type: mime })`. (Refinement over the spec's "Blob in Dexie" — simpler, equivalent.)
- **The existing text sync path is untouched:** `git_read_files`/`git_write_files` stay UTF-8 / `decks/`+`rem.json`+`tombstones.json` only. Assets travel exclusively through the new asset commands.
- **Supported image mimes:** `image/png`→`png`, `image/jpeg`→`jpg`, `image/gif`→`gif`, `image/webp`→`webp`; unknown mime → `bin`, unknown ext → `application/octet-stream`.
- **Commits are authored as `shettyh <manjunathshetty@live.com>`.**
- Run a single unit test file: `npx vitest run <path>`. Run a browser test: `npx vitest run <path.browser.test.tsx>`. Rust: `cargo test --manifest-path src-tauri/Cargo.toml <name>`. Typecheck: `npm run typecheck`.

---

### Task 1: SHA-256 asset hashing

**Files:**
- Create: `src/data/assetHash.ts`
- Test: `src/data/assetHash.test.ts`

**Interfaces:**
- Produces: `hashBytes(bytes: Uint8Array): Promise<string>` — 64-char lowercase hex SHA-256.

- [ ] **Step 1: Write the failing test**

```ts
// src/data/assetHash.test.ts
import { describe, it, expect } from 'vitest'
import { hashBytes } from './assetHash'

describe('hashBytes', () => {
  it('hashes bytes to the known SHA-256 hex of "abc"', async () => {
    const bytes = new TextEncoder().encode('abc')
    expect(await hashBytes(bytes)).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    )
  })

  it('is stable for identical bytes', async () => {
    const a = await hashBytes(new Uint8Array([1, 2, 3]))
    const b = await hashBytes(new Uint8Array([1, 2, 3]))
    expect(a).toBe(b)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/assetHash.test.ts`
Expected: FAIL — cannot find module `./assetHash`.

- [ ] **Step 3: Write minimal implementation**

```ts
// src/data/assetHash.ts
/** Content-addressed id for an asset: lowercase-hex SHA-256 of its bytes. */
export async function hashBytes(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/data/assetHash.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/data/assetHash.ts src/data/assetHash.test.ts
git commit -m "feat(assets): SHA-256 content hashing for assets"
```

---

### Task 2: Asset reference scanner

**Files:**
- Create: `src/data/assetRefs.ts`
- Test: `src/data/assetRefs.test.ts`

**Interfaces:**
- Produces: `assetRefs(markdown: string): string[]` — deduplicated hashes referenced as `asset:<hash>`.

- [ ] **Step 1: Write the failing test**

```ts
// src/data/assetRefs.test.ts
import { describe, it, expect } from 'vitest'
import { assetRefs } from './assetRefs'

const H = 'a'.repeat(64)
const H2 = 'b'.repeat(64)

describe('assetRefs', () => {
  it('extracts the hash from an image reference', () => {
    expect(assetRefs(`text ![pic](asset:${H}) more`)).toEqual([H])
  })

  it('deduplicates repeated references', () => {
    expect(assetRefs(`![a](asset:${H}) ![b](asset:${H})`)).toEqual([H])
  })

  it('returns distinct hashes and ignores non-asset urls', () => {
    expect(assetRefs(`![a](asset:${H}) ![b](asset:${H2}) [x](https://e.com)`).sort()).toEqual(
      [H, H2].sort(),
    )
  })

  it('returns empty for markdown with no assets', () => {
    expect(assetRefs('plain **markdown**')).toEqual([])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/assetRefs.test.ts`
Expected: FAIL — cannot find module `./assetRefs`.

- [ ] **Step 3: Write minimal implementation**

```ts
// src/data/assetRefs.ts
const ASSET_REF = /asset:([0-9a-f]{64})/g

/** Hashes referenced as `asset:<hash>` in markdown (e.g. `![alt](asset:<hash>)`). Deduplicated. */
export function assetRefs(markdown: string): string[] {
  const out = new Set<string>()
  for (const m of markdown.matchAll(ASSET_REF)) out.add(m[1])
  return [...out]
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/data/assetRefs.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/data/assetRefs.ts src/data/assetRefs.test.ts
git commit -m "feat(assets): scan markdown for asset references"
```

---

### Task 3: Asset domain type, Dexie table, putAsset/getAsset

**Files:**
- Modify: `src/domain/models.ts` (add `Asset`)
- Modify: `src/data/dexie/db.ts` (v4 `assets` table)
- Modify: `src/data/Storage.ts` (interface methods)
- Modify: `src/data/dexie/DexieStorage.ts` (implement)
- Test: `src/data/dexie/DexieStorage.test.ts` (append)

**Interfaces:**
- Consumes: `hashBytes` (Task 1).
- Produces:
  - `interface Asset { hash: string; mime: string; bytes: Uint8Array; createdAt: number }`
  - `Storage.putAsset(bytes: Uint8Array, mime: string): Promise<Asset>`
  - `Storage.getAsset(hash: string): Promise<Asset | undefined>`

- [ ] **Step 1: Write the failing test (append to DexieStorage.test.ts)**

```ts
describe('assets', () => {
  it('stores an asset and reads it back by hash', async () => {
    const asset = await storage.putAsset(new Uint8Array([1, 2, 3]), 'image/png')
    expect(asset.hash).toHaveLength(64)
    expect(asset.mime).toBe('image/png')
    const got = await storage.getAsset(asset.hash)
    expect(got?.bytes).toEqual(new Uint8Array([1, 2, 3]))
  })

  it('dedupes identical bytes to one record', async () => {
    const a = await storage.putAsset(new Uint8Array([9, 9]), 'image/png')
    const b = await storage.putAsset(new Uint8Array([9, 9]), 'image/png')
    expect(b.hash).toBe(a.hash)
    expect(await storage.db.assets.count()).toBe(1)
  })
})
```

Note: the test reaches `storage.db.assets`; expose it by changing `private readonly db` to `readonly db` in `DexieStorage` (Step 3).

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — `putAsset` is not a function.

- [ ] **Step 3: Implement**

In `src/domain/models.ts`, after the `Card` interface add:

```ts
/** A content-addressed binary asset (image/GIF) embedded in card markdown as `asset:<hash>`. */
export interface Asset {
  /** SHA-256 hex of {@link bytes}; primary key. */
  hash: ID
  /** MIME type, e.g. image/png. */
  mime: string
  bytes: Uint8Array
  createdAt: number
}
```

In `src/data/dexie/db.ts`:

```ts
import Dexie, { type EntityTable } from 'dexie'
import type { Asset, Card, Deck, Tombstone } from '../../domain/models'

export class RemDB extends Dexie {
  decks!: EntityTable<Deck, 'id'>
  cards!: EntityTable<Card, 'id'>
  tombstones!: EntityTable<Tombstone, 'id'>
  assets!: EntityTable<Asset, 'hash'>

  constructor(name = 'rem') {
    super(name)
    // ...keep v1, v2, v3 exactly as they are...
    // v4: add the assets table for embedded images. Additive — existing data untouched.
    this.version(4).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
  }
}
```

(Append the `assets!` field and the `this.version(4)…` block; leave v1–v3 untouched.)

In `src/data/Storage.ts`, add the import and three methods to the `Storage` interface:

```ts
import type { Asset, Card, Deck, ID, SchedulerKind, SchedulingState } from '../domain/models'
```

```ts
  // Assets (images/GIFs embedded in card markdown as asset:<hash>)
  putAsset(bytes: Uint8Array, mime: string): Promise<Asset>
  getAsset(hash: ID): Promise<Asset | undefined>
  sweepOrphanAssets(): Promise<void>
```

In `src/data/dexie/DexieStorage.ts`: change the constructor to `constructor(readonly db: RemDB) {}`, add the import, and add the methods:

```ts
import type { Asset, Card, Deck, ID, SchedulerKind } from '../../domain/models'
import { hashBytes } from '../assetHash'
```

```ts
  async putAsset(bytes: Uint8Array, mime: string): Promise<Asset> {
    const hash = await hashBytes(bytes)
    const existing = await this.db.assets.get(hash)
    if (existing) return existing
    const asset: Asset = { hash, mime, bytes, createdAt: Date.now() }
    await this.db.assets.add(asset)
    return asset
  }

  getAsset(hash: ID): Promise<Asset | undefined> {
    return this.db.assets.get(hash)
  }
```

(Add `sweepOrphanAssets` in Task 4 — declare it in the interface now; TypeScript will flag the missing impl, which Task 4 resolves. To keep this task green, add a temporary stub `async sweepOrphanAssets(): Promise<void> {}` and replace it in Task 4.)

- [ ] **Step 4: Run test + typecheck**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts && npm run typecheck`
Expected: PASS; typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add src/domain/models.ts src/data/dexie/db.ts src/data/Storage.ts src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat(assets): assets table with put/get behind Storage port"
```

---

### Task 4: sweepOrphanAssets

**Files:**
- Modify: `src/data/dexie/DexieStorage.ts` (replace the Task 3 stub)
- Test: `src/data/dexie/DexieStorage.test.ts` (append)

**Interfaces:**
- Consumes: `assetRefs` (Task 2), `Storage.createCard`/`deleteCard`.
- Produces: `Storage.sweepOrphanAssets(): Promise<void>` — deletes assets referenced by no card.

- [ ] **Step 1: Write the failing test (append to the `assets` describe block)**

```ts
it('sweeps assets not referenced by any card', async () => {
  const deck = await storage.createDeck('D')
  const used = await storage.putAsset(new Uint8Array([1]), 'image/png')
  const orphan = await storage.putAsset(new Uint8Array([2]), 'image/png')
  await storage.createCard(deck.id, `![x](asset:${used.hash})`, 'back')

  await storage.sweepOrphanAssets()

  expect(await storage.getAsset(used.hash)).toBeDefined()
  expect(await storage.getAsset(orphan.hash)).toBeUndefined()
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — orphan still defined (stub does nothing).

- [ ] **Step 3: Replace the stub**

```ts
import { assetRefs } from '../assetRefs'
```

```ts
  async sweepOrphanAssets(): Promise<void> {
    const [cards, assets] = await Promise.all([this.db.cards.toArray(), this.db.assets.toArray()])
    const referenced = new Set(cards.flatMap((c) => [...assetRefs(c.front), ...assetRefs(c.back)]))
    const orphans = assets.filter((a) => !referenced.has(a.hash)).map((a) => a.hash)
    if (orphans.length) await this.db.assets.bulkDelete(orphans)
  }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat(assets): sweep orphaned assets"
```

---

### Task 5: Snapshot assets + merge reconciliation

**Files:**
- Modify: `src/data/sync/snapshot.ts` (add `AssetBlob`, `RepoSnapshot.assets`, `EMPTY_SNAPSHOT`, deserialize)
- Modify: `src/data/sync/merge.ts` (`DbOps` asset fields, union+prune)
- Test: `src/data/sync/merge.test.ts` (append; update `snap` helper)

**Interfaces:**
- Consumes: `assetRefs` (Task 2).
- Produces:
  - `interface AssetBlob { hash: string; mime: string; bytes: Uint8Array }`
  - `RepoSnapshot.assets: AssetBlob[]`
  - `DbOps.upsertAssets: AssetBlob[]`, `DbOps.deleteAssetHashes: string[]`

- [ ] **Step 1: Update the `snap` helper and add tests in merge.test.ts**

Change the helper at the top of the file to include assets:

```ts
function snap(p: Partial<RepoSnapshot>): RepoSnapshot {
  return { decks: [], cards: [], tombstones: [], assets: [], ...p }
}
const H = 'a'.repeat(64)
function imgCard(id: string, hash: string): CardRecord {
  return {
    id, deckId: 'd1', front: `![x](asset:${hash})`, back: 'b', createdAt: 1, updatedAt: 10,
    scheduling: { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: 2.5, due: 0 },
  }
}
const blob = (hash: string): AssetBlob => ({ hash, mime: 'image/png', bytes: new Uint8Array([1]) })
```

Add `AssetBlob` to the import: `import type { RepoSnapshot, CardRecord, DeckRecord, AssetBlob } from './snapshot'`.

Append tests:

```ts
describe('merge assets', () => {
  it('keeps an asset referenced by a merged card', () => {
    const local = snap({ decks: [deck], cards: [imgCard('a', H)], assets: [blob(H)] })
    const { merged } = merge(local, snap({ decks: [deck] }))
    expect(merged.assets.map((a) => a.hash)).toEqual([H])
  })

  it('prunes an asset referenced by no merged card', () => {
    const local = snap({ decks: [deck], cards: [], assets: [blob(H)] })
    const { merged, dbOps } = merge(local, snap({}))
    expect(merged.assets).toEqual([])
    expect(dbOps.deleteAssetHashes).toEqual([H])
  })

  it('unions asset bytes from the remote side for a referenced card', () => {
    const remote = snap({ decks: [deck], cards: [imgCard('a', H)], assets: [blob(H)] })
    const { merged, dbOps } = merge(snap({ decks: [deck] }), remote)
    expect(merged.assets.map((a) => a.hash)).toEqual([H])
    expect(dbOps.upsertAssets.map((a) => a.hash)).toEqual([H])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/sync/merge.test.ts`
Expected: FAIL — `merged.assets` undefined / `AssetBlob` not exported.

- [ ] **Step 3: Implement**

In `src/data/sync/snapshot.ts`:

```ts
export interface AssetBlob {
  hash: string
  mime: string
  bytes: Uint8Array
}

export interface RepoSnapshot {
  decks: DeckRecord[]
  cards: CardRecord[]
  tombstones: Tombstone[]
  assets: AssetBlob[]
}

export const EMPTY_SNAPSHOT: RepoSnapshot = { decks: [], cards: [], tombstones: [], assets: [] }
```

In `deserializeSnapshot`, change the final return to include assets (bytes arrive via the GitBridge asset path, not text files):

```ts
  return { decks, cards, tombstones, assets: [] }
```

(`serializeSnapshot` is unchanged — it emits text files only; asset bytes are written via `GitBridge.writeAssets`.)

In `src/data/sync/merge.ts`:

```ts
import type { AssetBlob, CardRecord, DeckRecord, RepoSnapshot, Tombstone } from './snapshot'
import { assetRefs } from '../assetRefs'
```

Extend `DbOps`:

```ts
export interface DbOps {
  upsertDecks: DeckRecord[]
  upsertCards: CardRecord[]
  deleteDeckIds: string[]
  deleteCardIds: string[]
  tombstones: Tombstone[]
  upsertAssets: AssetBlob[]
  deleteAssetHashes: string[]
}
```

In `merge`, after `mergedCards` is built and before constructing `merged`, add:

```ts
  // Assets are immutable + content-addressed: union by hash, then keep only
  // those referenced by a surviving card. No last-writer-wins needed.
  const referencedHashes = new Set(
    mergedCards.flatMap((c) => [...assetRefs(c.front), ...assetRefs(c.back)]),
  )
  const assetByHash = new Map<string, AssetBlob>()
  for (const a of [...remote.assets, ...local.assets]) assetByHash.set(a.hash, a)
  const mergedAssets = [...assetByHash.values()].filter((a) => referencedHashes.has(a.hash))
```

Add `assets: mergedAssets` to the `merged` object, and to `dbOps` add:

```ts
    upsertAssets: mergedAssets,
    deleteAssetHashes: local.assets.filter((a) => !referencedHashes.has(a.hash)).map((a) => a.hash),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/data/sync/merge.test.ts && npm run typecheck`
Expected: PASS; typecheck clean (DexieStorage.applyMerge updated next task — if typecheck flags missing `upsertAssets` handling there it is fine until Task 6; if it blocks, add the two no-op lines from Task 6 now).

- [ ] **Step 5: Commit**

```bash
git add src/data/sync/snapshot.ts src/data/sync/merge.ts src/data/sync/merge.test.ts
git commit -m "feat(sync): reconcile assets by union + prune-to-referenced"
```

---

### Task 6: DexieStorage exportSnapshot/applyMerge assets

**Files:**
- Modify: `src/data/dexie/DexieStorage.ts`
- Test: `src/data/dexie/DexieStorage.test.ts` (append)

**Interfaces:**
- Consumes: `RepoSnapshot.assets`, `DbOps.upsertAssets`/`deleteAssetHashes` (Task 5).
- Produces: `exportSnapshot()` returns local assets; `applyMerge()` upserts/deletes assets.

- [ ] **Step 1: Write the failing test (append)**

```ts
describe('snapshot assets', () => {
  it('exports stored assets in the snapshot', async () => {
    const a = await storage.putAsset(new Uint8Array([7]), 'image/gif')
    const snap = await storage.exportSnapshot()
    expect(snap.assets.map((x) => x.hash)).toEqual([a.hash])
    expect(snap.assets[0].mime).toBe('image/gif')
  })

  it('applyMerge upserts new assets and deletes by hash', async () => {
    const keep = 'c'.repeat(64)
    await storage.applyMerge({
      upsertDecks: [], upsertCards: [], deleteDeckIds: [], deleteCardIds: [], tombstones: [],
      upsertAssets: [{ hash: keep, mime: 'image/png', bytes: new Uint8Array([5]) }],
      deleteAssetHashes: [],
    })
    expect((await storage.getAsset(keep))?.bytes).toEqual(new Uint8Array([5]))

    await storage.applyMerge({
      upsertDecks: [], upsertCards: [], deleteDeckIds: [], deleteCardIds: [], tombstones: [],
      upsertAssets: [], deleteAssetHashes: [keep],
    })
    expect(await storage.getAsset(keep)).toBeUndefined()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — `snap.assets` undefined / asset not persisted.

- [ ] **Step 3: Implement**

Replace `exportSnapshot`:

```ts
  async exportSnapshot(): Promise<RepoSnapshot> {
    const [decks, cards, tombstones, assets] = await Promise.all([
      this.db.decks.toArray(),
      this.db.cards.toArray(),
      this.db.tombstones.toArray(),
      this.db.assets.toArray(),
    ])
    return {
      decks,
      cards,
      tombstones,
      assets: assets.map(({ hash, mime, bytes }) => ({ hash, mime, bytes })),
    }
  }
```

In `applyMerge`, add the `assets` table to the transaction and the two ops (upserts get a fresh `createdAt`):

```ts
  async applyMerge(ops: DbOps): Promise<void> {
    await this.db.transaction(
      'rw',
      this.db.decks, this.db.cards, this.db.tombstones, this.db.assets,
      async () => {
        if (ops.deleteCardIds.length) await this.db.cards.bulkDelete(ops.deleteCardIds)
        if (ops.deleteDeckIds.length) await this.db.decks.bulkDelete(ops.deleteDeckIds)
        if (ops.deleteAssetHashes.length) await this.db.assets.bulkDelete(ops.deleteAssetHashes)
        if (ops.upsertDecks.length) await this.db.decks.bulkPut(ops.upsertDecks)
        if (ops.upsertCards.length) await this.db.cards.bulkPut(ops.upsertCards)
        if (ops.upsertAssets.length) {
          await this.db.assets.bulkPut(
            ops.upsertAssets.map((a) => ({ ...a, createdAt: Date.now() })),
          )
        }
        if (ops.tombstones.length) await this.db.tombstones.bulkPut(ops.tombstones)
      },
    )
  }
```

- [ ] **Step 4: Run test + typecheck**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts && npm run typecheck`
Expected: PASS; clean.

- [ ] **Step 5: Commit**

```bash
git add src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat(sync): carry assets through export/apply snapshot"
```

---

### Task 7: Asset file transport helpers (mime↔ext, base64)

**Files:**
- Create: `src/data/sync/assetFile.ts`
- Test: `src/data/sync/assetFile.test.ts`

**Interfaces:**
- Consumes: `AssetBlob` (Task 5).
- Produces:
  - `assetFileName(a: AssetBlob): string` → `<hash>.<ext>`
  - `assetFileToBlob(name: string, base64: string): AssetBlob`
  - `base64FromBytes(bytes: Uint8Array): string`, `base64ToBytes(b64: string): Uint8Array`

- [ ] **Step 1: Write the failing test**

```ts
// src/data/sync/assetFile.test.ts
import { describe, it, expect } from 'vitest'
import { assetFileName, assetFileToBlob, base64FromBytes, base64ToBytes } from './assetFile'

const H = 'a'.repeat(64)

describe('assetFile', () => {
  it('names a file <hash>.<ext> from mime', () => {
    expect(assetFileName({ hash: H, mime: 'image/png', bytes: new Uint8Array() })).toBe(`${H}.png`)
    expect(assetFileName({ hash: H, mime: 'image/jpeg', bytes: new Uint8Array() })).toBe(`${H}.jpg`)
  })

  it('round-trips bytes through base64', () => {
    const bytes = new Uint8Array([0, 1, 254, 255, 128])
    expect(base64ToBytes(base64FromBytes(bytes))).toEqual(bytes)
  })

  it('reconstructs a blob from filename + base64', () => {
    const bytes = new Uint8Array([10, 20, 30])
    const blob = assetFileToBlob(`${H}.gif`, base64FromBytes(bytes))
    expect(blob).toEqual({ hash: H, mime: 'image/gif', bytes })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/sync/assetFile.test.ts`
Expected: FAIL — cannot find module `./assetFile`.

- [ ] **Step 3: Implement**

```ts
// src/data/sync/assetFile.ts
import type { AssetBlob } from './snapshot'

const MIME_EXT: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
}
const EXT_MIME: Record<string, string> = Object.fromEntries(
  Object.entries(MIME_EXT).map(([mime, ext]) => [ext, mime]),
)

/** On-disk filename for an asset: `<hash>.<ext>`. Unknown mime → `bin`. */
export function assetFileName(a: AssetBlob): string {
  return `${a.hash}.${MIME_EXT[a.mime] ?? 'bin'}`
}

/** Inverse of {@link assetFileName} + base64 payload → an AssetBlob. */
export function assetFileToBlob(name: string, base64: string): AssetBlob {
  const dot = name.lastIndexOf('.')
  const hash = dot >= 0 ? name.slice(0, dot) : name
  const ext = dot >= 0 ? name.slice(dot + 1) : ''
  return { hash, mime: EXT_MIME[ext] ?? 'application/octet-stream', bytes: base64ToBytes(base64) }
}

export function base64FromBytes(bytes: Uint8Array): string {
  let bin = ''
  for (const b of bytes) bin += String.fromCharCode(b)
  return btoa(bin)
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/data/sync/assetFile.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/data/sync/assetFile.ts src/data/sync/assetFile.test.ts
git commit -m "feat(sync): asset file naming + base64 transport helpers"
```

---

### Task 8: GitBridge asset methods + FakeGitBridge

**Files:**
- Modify: `src/data/sync/GitBridge.ts` (interface)
- Modify: `src/data/sync/FakeGitBridge.ts` (in-memory impl)
- Test: `src/data/sync/FakeGitBridge.test.ts` (append)

**Interfaces:**
- Consumes: `AssetBlob` (Task 5).
- Produces:
  - `GitBridge.readAssets(dir: string): Promise<AssetBlob[]>`
  - `GitBridge.writeAssets(dir: string, assets: AssetBlob[]): Promise<void>`

- [ ] **Step 1: Write the failing test (append)**

```ts
import type { AssetBlob } from './snapshot'

const blob = (h: string): AssetBlob => ({ hash: h, mime: 'image/png', bytes: new Uint8Array([1, 2]) })

describe('FakeGitBridge assets', () => {
  it('writes and reads assets in the working copy', async () => {
    const bridge = new FakeGitBridge(null)
    await bridge.clone('url', '/d')
    await bridge.writeAssets('/d', [blob('a'.repeat(64))])
    const read = await bridge.readAssets('/d')
    expect(read.map((a) => a.hash)).toEqual(['a'.repeat(64)])
  })

  it('replaces the asset set on write (delete-absent)', async () => {
    const bridge = new FakeGitBridge(null)
    await bridge.clone('url', '/d')
    await bridge.writeAssets('/d', [blob('a'.repeat(64)), blob('b'.repeat(64))])
    await bridge.writeAssets('/d', [blob('a'.repeat(64))])
    expect((await bridge.readAssets('/d')).map((a) => a.hash)).toEqual(['a'.repeat(64)])
  })

  it('publishes assets to the remote on commitPush', async () => {
    const bridge = new FakeGitBridge(null)
    await bridge.clone('url', '/d')
    await bridge.fetchReset('/d')
    await bridge.writeAssets('/d', [blob('a'.repeat(64))])
    await bridge.commitPush('/d', 'msg')
    expect(bridge.remoteAssets.map((a) => a.hash)).toEqual(['a'.repeat(64)])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/sync/FakeGitBridge.test.ts`
Expected: FAIL — `writeAssets` not a function.

- [ ] **Step 3: Implement**

In `src/data/sync/GitBridge.ts` add the import and two methods to the interface:

```ts
import type { AssetBlob } from './snapshot'
```

```ts
  /** Binary asset files under assets/, as content-addressed blobs. */
  readAssets(dir: string): Promise<AssetBlob[]>
  /** Replace the assets/ set with `assets` (delete-absent), matching writeFiles. */
  writeAssets(dir: string, assets: AssetBlob[]): Promise<void>
```

In `src/data/sync/FakeGitBridge.ts` add the import, fields, and methods, and publish assets on push:

```ts
import type { AssetBlob } from './snapshot'
```

Add fields alongside the existing ones:

```ts
  remoteAssets: AssetBlob[] = []
  private workingAssets: AssetBlob[] = []
```

In `clone`, after setting `this.working`, add: `this.workingAssets = [...this.remoteAssets]`.
In `fetchReset`, after setting `this.working`, add: `this.workingAssets = [...this.remoteAssets]`.
In `commitPush`, where the working copy is published to the remote on success (`this.remote = { ...this.working }`), also add: `this.remoteAssets = [...this.workingAssets]`.

Add the two methods:

```ts
  async readAssets(_dir: string): Promise<AssetBlob[]> {
    return [...this.workingAssets]
  }

  async writeAssets(_dir: string, assets: AssetBlob[]): Promise<void> {
    this.workingAssets = [...assets]
  }
```

- [ ] **Step 4: Run test + typecheck**

Run: `npx vitest run src/data/sync/FakeGitBridge.test.ts && npm run typecheck`
Expected: PASS; clean.

- [ ] **Step 5: Commit**

```bash
git add src/data/sync/GitBridge.ts src/data/sync/FakeGitBridge.ts src/data/sync/FakeGitBridge.test.ts
git commit -m "feat(sync): GitBridge asset transport + fake impl"
```

---

### Task 9: GitSyncService asset wiring

**Files:**
- Modify: `src/data/sync/GitSyncService.ts`
- Test: `src/data/sync/GitSyncService.test.ts` (append)

**Interfaces:**
- Consumes: `GitBridge.readAssets`/`writeAssets` (Task 8), `RepoSnapshot.assets` (Task 5).
- Produces: `sync()` round-trips asset bytes alongside text files.

- [ ] **Step 1: Write the failing test (append)**

Mirror the existing GitSyncService test setup (two `DexieStorage` instances + one `FakeGitBridge`). Prove an image card syncs A→B:

```ts
it('syncs an image asset from one machine to another', async () => {
  const bridge = new FakeGitBridge(null)
  // Machine A: a deck with a card embedding an asset.
  const deck = await storageA.createDeck('D', 'fsrs')
  const asset = await storageA.putAsset(new Uint8Array([3, 1, 4]), 'image/png')
  await storageA.createCard(deck.id, `![x](asset:${asset.hash})`, 'back')
  await new GitSyncService(storageA, bridge, cfg).sync()

  // Machine B: a fresh store syncs from the same remote.
  await new GitSyncService(storageB, bridge, cfg).sync()

  const got = await storageB.getAsset(asset.hash)
  expect(got?.bytes).toEqual(new Uint8Array([3, 1, 4]))
})
```

(Use the file's existing helpers for `storageA`, `storageB`, and `cfg`; if it only sets up one storage, add a second `DexieStorage` on a distinct DB name in `beforeEach`, following the Task-3 test's `new RemDB('name')` pattern.)

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/data/sync/GitSyncService.test.ts`
Expected: FAIL — asset undefined on machine B (not transported).

- [ ] **Step 3: Implement**

In `src/data/sync/GitSyncService.ts`, read assets with the remote snapshot and write them after the text files:

```ts
      const remote = remoteExists
        ? {
            ...deserializeSnapshot(await this.bridge.readFiles(repoDir)),
            assets: await this.bridge.readAssets(repoDir),
          }
        : EMPTY_SNAPSHOT
      const local = await this.storage.exportSnapshot()
      const { merged, dbOps } = merge(local, remote)
      await this.storage.applyMerge(dbOps)
      await this.bridge.writeFiles(repoDir, serializeSnapshot(merged))
      await this.bridge.writeAssets(repoDir, merged.assets)
```

- [ ] **Step 4: Run test + typecheck**

Run: `npx vitest run src/data/sync/GitSyncService.test.ts && npm run typecheck`
Expected: PASS; clean.

- [ ] **Step 5: Commit**

```bash
git add src/data/sync/GitSyncService.ts src/data/sync/GitSyncService.test.ts
git commit -m "feat(sync): transport assets through the sync cycle"
```

---

### Task 10: Rust git_read_assets / git_write_assets

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `base64`)
- Modify: `src-tauri/src/git.rs` (commands + test)
- Modify: `src-tauri/src/lib.rs` (register)

**Interfaces:**
- Produces Tauri commands:
  - `git_read_assets(dir) -> Vec<AssetFile { name, data /*base64*/ }>`
  - `git_write_assets(dir, assets: Vec<AssetFile>)` — clears `assets/` then writes raw bytes.

- [ ] **Step 1: Write the failing Rust test (append to the `tests` module in git.rs)**

```rust
#[test]
fn test_write_read_assets_binary_roundtrip_and_delete_absent() {
    let dir = make_temp_dir();
    let dir_str = dir.to_string_lossy().to_string();

    // Non-UTF8 bytes must survive the round trip.
    let bytes = vec![0u8, 1, 254, 255, 128];
    let b64 = STANDARD.encode(&bytes);
    let assets = vec![
        AssetFile { name: "aaaa.png".into(), data: b64.clone() },
        AssetFile { name: "bbbb.gif".into(), data: b64.clone() },
    ];
    git_write_assets(dir_str.clone(), assets).unwrap();

    let read = git_read_assets(dir_str.clone()).unwrap();
    assert_eq!(read.len(), 2);
    let aaaa = read.iter().find(|a| a.name == "aaaa.png").unwrap();
    assert_eq!(STANDARD.decode(&aaaa.data).unwrap(), bytes);

    // Second write with only one asset deletes the other.
    git_write_assets(dir_str.clone(), vec![AssetFile { name: "aaaa.png".into(), data: b64 }]).unwrap();
    let read2 = git_read_assets(dir_str.clone()).unwrap();
    assert_eq!(read2.len(), 1);
    assert_eq!(read2[0].name, "aaaa.png");

    fs::remove_dir_all(&dir).unwrap();
}
```

Add to the top of git.rs: `use base64::{engine::general_purpose::STANDARD, Engine};` and `use serde::{Deserialize, Serialize};` (replacing the existing `use serde::Serialize;`).

- [ ] **Step 2: Add the base64 dependency, then run to verify it fails**

Add under `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
base64 = "0.22"
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml test_write_read_assets`
Expected: FAIL — `AssetFile` / `git_write_assets` undefined.

- [ ] **Step 3: Implement (add to git.rs above the tests module)**

```rust
#[derive(Serialize, Deserialize)]
pub struct AssetFile {
    name: String,
    /// base64-encoded file bytes (transport only; on disk the bytes are raw).
    data: String,
}

#[tauri::command]
pub async fn git_read_assets(dir: String) -> Result<Vec<AssetFile>, String> {
    let assets_dir = Path::new(&dir).join("assets");
    let mut out = Vec::new();
    if !assets_dir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(&assets_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        out.push(AssetFile { name, data: STANDARD.encode(&bytes) });
    }
    Ok(out)
}

#[tauri::command]
pub async fn git_write_assets(dir: String, assets: Vec<AssetFile>) -> Result<(), String> {
    let assets_dir = Path::new(&dir).join("assets");
    let _ = fs::remove_dir_all(&assets_dir);
    if !assets.is_empty() {
        fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    }
    for a in assets {
        let bytes = STANDARD.decode(&a.data).map_err(|e| e.to_string())?;
        fs::write(assets_dir.join(&a.name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

Register both in `src-tauri/src/lib.rs` inside `generate_handler!`:

```rust
            git::git_read_files,
            git::git_write_files,
            git::git_read_assets,
            git::git_write_assets,
            git::git_commit_push,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml test_write_read_assets`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/git.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): read/write binary asset files for sync"
```

---

### Task 11: TauriGitBridge asset methods

**Files:**
- Modify: `src/data/sync/TauriGitBridge.ts`

**Interfaces:**
- Consumes: `assetFileName`, `assetFileToBlob`, `base64FromBytes` (Task 7); Rust commands (Task 10).
- Produces: `TauriGitBridge.readAssets`/`writeAssets` implementing the `GitBridge` additions.

(No unit test — `TauriGitBridge` is the real IPC adapter and, like the rest of the file, is covered by the Rust tests + manual run, not vitest. Verification is `npm run typecheck`.)

- [ ] **Step 1: Implement**

```ts
import { invoke } from '@tauri-apps/api/core'
import type { CommitPushResult, FetchResetResult, GitBridge } from './GitBridge'
import type { AssetBlob } from './snapshot'
import { assetFileName, assetFileToBlob, base64FromBytes } from './assetFile'
```

Add the two methods to the class:

```ts
  async readAssets(dir: string): Promise<AssetBlob[]> {
    const files = await invoke<{ name: string; data: string }[]>('git_read_assets', { dir })
    return files.map((f) => assetFileToBlob(f.name, f.data))
  }

  async writeAssets(dir: string, assets: AssetBlob[]): Promise<void> {
    const files = assets.map((a) => ({ name: assetFileName(a), data: base64FromBytes(a.bytes) }))
    await invoke<void>('git_write_assets', { dir, files })
  }
```

- [ ] **Step 2: Verify typecheck**

Run: `npm run typecheck`
Expected: clean (TauriGitBridge now fully implements GitBridge).

- [ ] **Step 3: Commit**

```bash
git add src/data/sync/TauriGitBridge.ts
git commit -m "feat(sync): TauriGitBridge asset read/write via IPC"
```

---

### Task 12: Asset URL resolver + useAssetUrl hook

**Files:**
- Create: `src/features/cards/assetUrl.ts`
- Test: `src/features/cards/assetUrl.test.ts`

**Interfaces:**
- Consumes: `Storage.getAsset` (Task 3), `useStorage` (existing).
- Produces:
  - `loadAssetUrl(storage: Storage, hash: string): Promise<string | null>` — object URL or null.
  - `useAssetUrl(hash: string | undefined): string | null` — React hook; revokes on unmount/change.

- [ ] **Step 1: Write the failing test**

```ts
// src/features/cards/assetUrl.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { loadAssetUrl } from './assetUrl'

beforeEach(async () => {
  await Dexie.delete('rem-asseturl')
})

describe('loadAssetUrl', () => {
  it('returns an object URL built from the asset blob', async () => {
    const storage = new DexieStorage(new RemDB('rem-asseturl'))
    const asset = await storage.putAsset(new Uint8Array([1, 2, 3]), 'image/png')
    const createObjectURL = vi.fn(() => 'blob:fake')
    vi.stubGlobal('URL', { ...URL, createObjectURL })

    const url = await loadAssetUrl(storage, asset.hash)

    expect(url).toBe('blob:fake')
    const blobArg = createObjectURL.mock.calls[0][0] as Blob
    expect(blobArg.type).toBe('image/png')
    expect(blobArg.size).toBe(3)
    vi.unstubAllGlobals()
  })

  it('returns null for a missing asset', async () => {
    const storage = new DexieStorage(new RemDB('rem-asseturl'))
    expect(await loadAssetUrl(storage, 'd'.repeat(64))).toBeNull()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/features/cards/assetUrl.test.ts`
Expected: FAIL — cannot find module `./assetUrl`.

- [ ] **Step 3: Implement**

```ts
// src/features/cards/assetUrl.ts
import { useEffect, useState } from 'react'
import type { Storage } from '../../data/Storage'
import { useStorage } from '../../data/StorageContext'

/** Resolve an asset hash to an object URL, or null if absent. Caller owns revocation. */
export async function loadAssetUrl(storage: Storage, hash: string): Promise<string | null> {
  const asset = await storage.getAsset(hash)
  if (!asset) return null
  return URL.createObjectURL(new Blob([asset.bytes], { type: asset.mime }))
}

/** Object URL for an asset hash, revoked on unmount or when the hash changes. */
export function useAssetUrl(hash: string | undefined): string | null {
  const storage = useStorage()
  const [url, setUrl] = useState<string | null>(null)

  useEffect(() => {
    if (!hash) {
      setUrl(null)
      return
    }
    let revoked = false
    let current: string | null = null
    void loadAssetUrl(storage, hash).then((resolved) => {
      if (revoked) {
        if (resolved) URL.revokeObjectURL(resolved)
        return
      }
      current = resolved
      setUrl(resolved)
    })
    return () => {
      revoked = true
      if (current) URL.revokeObjectURL(current)
    }
  }, [storage, hash])

  return url
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/features/cards/assetUrl.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/cards/assetUrl.ts src/features/cards/assetUrl.test.ts
git commit -m "feat(cards): resolve asset hashes to object URLs"
```

---

### Task 13: MarkdownView renders asset images

**Files:**
- Modify: `src/features/cards/MarkdownView.tsx`
- Test: `src/features/cards/MarkdownView.browser.test.tsx` (create)

**Interfaces:**
- Consumes: `useAssetUrl` (Task 12), `StorageProvider` (existing).
- Produces: `MarkdownView` resolves `asset:<hash>` image srcs to object URLs.

- [ ] **Step 1: Write the failing browser test**

```tsx
// src/features/cards/MarkdownView.browser.test.tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render } from 'vitest-browser-react'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { MarkdownView } from './MarkdownView'

beforeEach(async () => {
  await Dexie.delete('rem-mdview')
})

describe('MarkdownView assets', () => {
  it('renders an asset image as a blob-backed <img>', async () => {
    const storage = new DexieStorage(new RemDB('rem-mdview'))
    const asset = await storage.putAsset(new Uint8Array([137, 80, 78, 71]), 'image/png')
    const { container } = render(
      <StorageProvider storage={storage}>
        <MarkdownView source={`![pic](asset:${asset.hash})`} />
      </StorageProvider>,
    )
    await expect.poll(() => container.querySelector('img')?.getAttribute('src')).toMatch(/^blob:/)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/features/cards/MarkdownView.browser.test.tsx`
Expected: FAIL — img src stays `asset:…`, never `blob:`.

- [ ] **Step 3: Implement**

```tsx
// src/features/cards/MarkdownView.tsx
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import 'highlight.js/styles/github-dark.css'
import { useAssetUrl } from './assetUrl'

const ASSET_SRC = /^asset:([0-9a-f]{64})$/

/** An <img> whose `asset:<hash>` src resolves to an object URL; plain srcs pass through. */
function MarkdownImg({ src, alt }: { src?: string; alt?: string }) {
  const match = src ? ASSET_SRC.exec(src) : null
  const resolved = useAssetUrl(match?.[1])
  const finalSrc = match ? (resolved ?? undefined) : src
  return <img src={finalSrc} alt={alt ?? ''} />
}

/** Renders markdown source (text, code with highlighting, lists, images, etc.). */
export function MarkdownView({ source }: { source: string }) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{ img: MarkdownImg }}
      >
        {source}
      </ReactMarkdown>
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/features/cards/MarkdownView.browser.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/features/cards/MarkdownView.tsx src/features/cards/MarkdownView.browser.test.tsx
git commit -m "feat(cards): render asset images in MarkdownView"
```

---

### Task 14: Image extension + node-view in the editor

**Files:**
- Modify: `package.json` / `package-lock.json` (add `@tiptap/extension-image`)
- Create: `src/features/cards/imageExtension.ts`
- Modify: `src/features/cards/editorExtensions.ts` (accept a resolver, add Image)
- Modify: `src/features/cards/RichMarkdownEditor.tsx` (build the resolver from props)
- Modify: `src/features/cards/editorExtensions.test.ts` (append)

**Interfaces:**
- Consumes: `loadAssetUrl` (Task 12).
- Produces:
  - `createImageExtension(resolveAsset?: (hash: string) => Promise<string | null>)` — TipTap Image with a DOM node-view.
  - `createEditorExtensions(placeholder?, resolveAsset?)` now includes the image extension.
  - `RichMarkdownEditor` gains props `resolveAsset?` and (Task 16) `ingestImage?`.

- [ ] **Step 1: Install the dependency**

Run: `npm install @tiptap/extension-image@^3.27.1`
Expected: adds `@tiptap/extension-image` to dependencies.

- [ ] **Step 2: Write the failing test (append to editorExtensions.test.ts)**

```ts
import { createEditorExtensions } from './editorExtensions'

it('includes an image extension', () => {
  const names = createEditorExtensions('', undefined).map((e) => e.name)
  expect(names).toContain('image')
})
```

Run: `npx vitest run src/features/cards/editorExtensions.test.ts`
Expected: FAIL — no `image` in the extension names.

- [ ] **Step 3: Implement the image extension**

```ts
// src/features/cards/imageExtension.ts
import Image from '@tiptap/extension-image'

const ASSET_SRC = /^asset:([0-9a-f]{64})$/

/** TipTap Image with a DOM node-view that resolves `asset:<hash>` srcs to object URLs. */
export function createImageExtension(resolveAsset?: (hash: string) => Promise<string | null>) {
  return Image.extend({
    addNodeView() {
      return ({ node }) => {
        const dom = document.createElement('img')
        const src: string = node.attrs.src ?? ''
        if (node.attrs.alt) dom.alt = node.attrs.alt as string
        const match = ASSET_SRC.exec(src)
        if (match && resolveAsset) {
          void resolveAsset(match[1]).then((url) => {
            if (url) dom.src = url
          })
        } else {
          dom.src = src
        }
        return { dom }
      }
    },
  })
}
```

Update `src/features/cards/editorExtensions.ts`:

```ts
import { createImageExtension } from './imageExtension'
```

```ts
export function createEditorExtensions(
  placeholder?: string,
  resolveAsset?: (hash: string) => Promise<string | null>,
): Extensions {
  return [
    StarterKit.configure({
      codeBlock: false,
      heading: { levels: [1, 2, 3] },
    }),
    CodeBlockLowlight.configure({ lowlight }),
    createImageExtension(resolveAsset),
    Markdown.configure({ transformPastedText: true }),
    Placeholder.configure({ placeholder: placeholder ?? '' }),
  ]
}
```

Update `src/features/cards/RichMarkdownEditor.tsx` to accept and thread a resolver:

```ts
export function RichMarkdownEditor({
  value,
  onChange,
  placeholder,
  ariaLabel,
  resolveAsset,
}: {
  value: string
  onChange: (markdown: string) => void
  placeholder?: string
  ariaLabel?: string
  resolveAsset?: (hash: string) => Promise<string | null>
}) {
```

In the `useEditor` config, change `extensions: createEditorExtensions(placeholder)` to `extensions: createEditorExtensions(placeholder, resolveAsset)`.

- [ ] **Step 4: Run test + typecheck**

Run: `npx vitest run src/features/cards/editorExtensions.test.ts && npm run typecheck`
Expected: PASS; clean.

- [ ] **Step 5: Commit**

```bash
git add package.json package-lock.json src/features/cards/imageExtension.ts src/features/cards/editorExtensions.ts src/features/cards/editorExtensions.test.ts src/features/cards/RichMarkdownEditor.tsx
git commit -m "feat(cards): TipTap image node-view resolving asset refs"
```

---

### Task 15: Persistent toolbar (replace bubble menu)

**Files:**
- Modify: `src/features/cards/RichMarkdownEditor.tsx`
- Modify: `src/ui/styles.css` (toolbar styles)
- Test: `src/features/cards/RichEditorToolbar.browser.test.tsx` (create)

**Interfaces:**
- Produces: a persistent `.editor-toolbar` above the content with formatting buttons. The image button (`aria-label="Image"`) is wired in Task 16.

- [ ] **Step 1: Write the failing browser test**

```tsx
// src/features/cards/RichEditorToolbar.browser.test.tsx
import { describe, it, expect } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from '@vitest/browser/context'
import { useState } from 'react'
import { RichMarkdownEditor } from './RichMarkdownEditor'

function Harness() {
  const [v, setV] = useState('')
  return (
    <>
      <RichMarkdownEditor value={v} onChange={setV} ariaLabel="Front" />
      <pre data-testid="md">{v}</pre>
    </>
  )
}

describe('editor toolbar', () => {
  it('bold turns typed text bold via the toolbar', async () => {
    const { container, getByTestId } = render(<Harness />)
    const content = container.querySelector('.rich-editor-content') as HTMLElement
    content.focus()
    // Toggle bold on (collapsed selection sets the stored mark), then type.
    await userEvent.click(container.querySelector('[aria-label="Bold"]')!)
    await userEvent.type(content, 'x')
    await expect.poll(() => getByTestId('md').textContent).toContain('**x**')
  })

  it('exposes an image button', () => {
    const { container } = render(<RichMarkdownEditor value="" onChange={() => {}} />)
    expect(container.querySelector('[aria-label="Image"]')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/features/cards/RichEditorToolbar.browser.test.tsx`
Expected: FAIL — no `[aria-label="Bold"]` toolbar button (only the bubble menu exists).

- [ ] **Step 3: Implement — replace the bubble menu with a toolbar**

In `src/features/cards/RichMarkdownEditor.tsx`, remove the `BubbleMenu` import and its JSX block, and render a toolbar instead. Replace the returned JSX with:

```tsx
  function setLink() {
    if (!editor) return
    if (editor.isActive('link')) {
      editor.chain().focus().unsetLink().run()
      return
    }
    const url = window.prompt('URL')
    if (url) editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
  }

  return (
    <div className="rich-editor">
      {editor && (
        <div className="editor-toolbar" role="toolbar" aria-label="Formatting">
          <button type="button" aria-label="Heading 1" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('heading', { level: 1 }) ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}>H1</button>
          <button type="button" aria-label="Heading 2" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('heading', { level: 2 }) ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}>H2</button>
          <button type="button" aria-label="Heading 3" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('heading', { level: 3 }) ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}>H3</button>
          <span className="toolbar-sep" />
          <button type="button" aria-label="Bold" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('bold') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleBold().run()}>B</button>
          <button type="button" aria-label="Italic" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('italic') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleItalic().run()}>i</button>
          <button type="button" aria-label="Inline code" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('code') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleCode().run()}>{'</>'}</button>
          <button type="button" aria-label="Code block" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('codeBlock') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleCodeBlock().run()}>{'{ }'}</button>
          <span className="toolbar-sep" />
          <button type="button" aria-label="Bullet list" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('bulletList') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleBulletList().run()}>•</button>
          <button type="button" aria-label="Numbered list" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('orderedList') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleOrderedList().run()}>1.</button>
          <button type="button" aria-label="Quote" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('blockquote') ? 'active' : ''}
            onClick={() => editor.chain().focus().toggleBlockquote().run()}>&ldquo;</button>
          <span className="toolbar-sep" />
          <button type="button" aria-label="Link" onMouseDown={(e) => e.preventDefault()}
            className={editor.isActive('link') ? 'active' : ''} onClick={setLink}>link</button>
          <button type="button" aria-label="Image" onMouseDown={(e) => e.preventDefault()}
            onClick={() => {}}>img</button>
        </div>
      )}
      <EditorContent editor={editor} />
    </div>
  )
```

(The image button's `onClick` is a no-op placeholder filled in Task 16.)

Add to `src/ui/styles.css`:

```css
.editor-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px;
  padding: 6px;
  border-bottom: 1px solid var(--border, #2a2a2a);
}
.editor-toolbar button {
  min-width: 28px;
  height: 28px;
  padding: 0 6px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  font: inherit;
}
.editor-toolbar button:hover { background: var(--hover, rgba(255, 255, 255, 0.08)); }
.editor-toolbar button.active { background: var(--accent-soft, rgba(120, 160, 255, 0.25)); }
.toolbar-sep { width: 1px; height: 18px; margin: 0 4px; background: var(--border, #2a2a2a); }
```

- [ ] **Step 4: Run test + typecheck**

Run: `npx vitest run src/features/cards/RichEditorToolbar.browser.test.tsx && npm run typecheck`
Expected: PASS; clean.

- [ ] **Step 5: Commit**

```bash
git add src/features/cards/RichMarkdownEditor.tsx src/ui/styles.css src/features/cards/RichEditorToolbar.browser.test.tsx
git commit -m "feat(cards): persistent editor toolbar, drop bubble menu"
```

---

### Task 16: Image ingestion (button, paste, drop)

**Files:**
- Modify: `src/features/cards/RichMarkdownEditor.tsx`
- Test: `src/features/cards/RichEditorIngest.browser.test.tsx` (create)

**Interfaces:**
- Consumes: `Image` command `setImage` (Task 14).
- Produces: `RichMarkdownEditor` prop `ingestImage?: (file: File) => Promise<{ hash: string; mime: string }>`; toolbar image button opens a file picker; paste and drop of image files insert `asset:<hash>` nodes.

- [ ] **Step 1: Write the failing browser test**

```tsx
// src/features/cards/RichEditorIngest.browser.test.tsx
import { describe, it, expect } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from '@vitest/browser/context'
import { useState } from 'react'
import { RichMarkdownEditor } from './RichMarkdownEditor'

const HASH = 'a'.repeat(64)

function Harness() {
  const [v, setV] = useState('')
  const ingestImage = async (_file: File) => ({ hash: HASH, mime: 'image/png' })
  return (
    <>
      <RichMarkdownEditor value={v} onChange={setV} ariaLabel="Front" ingestImage={ingestImage} />
      <pre data-testid="md">{v}</pre>
    </>
  )
}

describe('image ingestion', () => {
  it('inserts an asset image when a file is chosen', async () => {
    const { container, getByTestId } = render(<Harness />)
    const input = container.querySelector('input[type="file"]') as HTMLInputElement
    const file = new File([new Uint8Array([1, 2, 3])], 'p.png', { type: 'image/png' })
    await userEvent.upload(input, file)
    await expect.poll(() => getByTestId('md').textContent).toContain(`asset:${HASH}`)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/features/cards/RichEditorIngest.browser.test.tsx`
Expected: FAIL — no file input / no asset ref inserted.

- [ ] **Step 3: Implement**

In `src/features/cards/RichMarkdownEditor.tsx`:

Add `ingestImage` to the props (alongside `resolveAsset`):

```ts
  ingestImage,
}: {
  // ...existing props...
  ingestImage?: (file: File) => Promise<{ hash: string; mime: string }>
}) {
```

Add a ref so editor-prop handlers can call the latest insert logic, plus the file-input ref:

```ts
import { useEffect, useRef } from 'react'
```

```ts
  const fileInputRef = useRef<HTMLInputElement>(null)
  const insertImageRef = useRef<(file: File) => void>(() => {})
```

After `useEditor(...)`, define the insert function and keep the ref current:

```ts
  insertImageRef.current = (file: File) => {
    if (!editor || !ingestImage || !file.type.startsWith('image/')) return
    void ingestImage(file).then(({ hash }) => {
      editor.chain().focus().setImage({ src: `asset:${hash}` }).run()
    })
  }
```

Add paste/drop handling to the `useEditor` `editorProps`:

```ts
    editorProps: {
      attributes: {
        class: 'rich-editor-content',
        ...(ariaLabel ? { 'aria-label': ariaLabel } : {}),
      },
      handlePaste: (_view, event) => {
        const file = [...(event.clipboardData?.items ?? [])]
          .find((i) => i.type.startsWith('image/'))?.getAsFile()
        if (file) {
          insertImageRef.current(file)
          return true
        }
        return false
      },
      handleDrop: (_view, event) => {
        const file = [...(event.dataTransfer?.files ?? [])].find((f) => f.type.startsWith('image/'))
        if (file) {
          event.preventDefault()
          insertImageRef.current(file)
          return true
        }
        return false
      },
    },
```

Wire the toolbar image button to the hidden input, and render the input. Replace the image button's `onClick={() => {}}` with `onClick={() => fileInputRef.current?.click()}`, and add the input just before `<EditorContent …>`:

```tsx
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        style={{ display: 'none' }}
        onChange={(e) => {
          const file = e.target.files?.[0]
          if (file) insertImageRef.current(file)
          e.target.value = ''
        }}
      />
      <EditorContent editor={editor} />
```

- [ ] **Step 4: Run test + typecheck**

Run: `npx vitest run src/features/cards/RichEditorIngest.browser.test.tsx && npm run typecheck`
Expected: PASS; clean.

- [ ] **Step 5: Commit**

```bash
git add src/features/cards/RichMarkdownEditor.tsx src/features/cards/RichEditorIngest.browser.test.tsx
git commit -m "feat(cards): ingest images via button, paste, and drop"
```

---

### Task 17: CardEditorPage route (replace the modal)

**Files:**
- Create: `src/features/cards/CardEditorPage.tsx`
- Modify: `src/app/routes.tsx` (two routes)
- Modify: `src/features/cards/DeckDetailPage.tsx` (navigate instead of modal)
- Delete: `src/features/cards/CardEditorModal.tsx`
- Modify: `src/ui/styles.css` (remove orphaned `.modal-*` rules; add editor-page layout)
- Test: `src/features/cards/CardEditorPage.browser.test.tsx` (create)

**Interfaces:**
- Consumes: `RichMarkdownEditor` (`resolveAsset`, `ingestImage`), `useStorage`, `loadAssetUrl`, react-router `useParams`/`useNavigate`.
- Produces: routes `/decks/:deckId/cards/new` and `/decks/:deckId/cards/:cardId/edit`.

- [ ] **Step 1: Write the failing browser test**

```tsx
// src/features/cards/CardEditorPage.browser.test.tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render } from 'vitest-browser-react'
import { userEvent } from '@vitest/browser/context'
import { createMemoryRouter, RouterProvider } from 'react-router-dom'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { StorageProvider } from '../../data/StorageContext'
import { CardEditorPage } from './CardEditorPage'

beforeEach(async () => {
  await Dexie.delete('rem-editorpage')
})

function renderAt(storage: DexieStorage, path: string) {
  const router = createMemoryRouter(
    [
      { path: '/decks/:deckId/cards/new', element: <CardEditorPage /> },
      { path: '/decks/:deckId', element: <div>deck page</div> },
    ],
    { initialEntries: [path] },
  )
  return render(
    <StorageProvider storage={storage}>
      <RouterProvider router={router} />
    </StorageProvider>,
  )
}

describe('CardEditorPage', () => {
  it('creates a card and navigates back to the deck', async () => {
    const storage = new DexieStorage(new RemDB('rem-editorpage'))
    const deck = await storage.createDeck('D', 'fsrs')
    const screen = renderAt(storage, `/decks/${deck.id}/cards/new`)

    const front = screen.container.querySelector('[aria-label="Front"]') as HTMLElement
    front.focus()
    await userEvent.type(front, 'Capital of France')
    await userEvent.click(screen.getByText('Save'))

    await expect.poll(async () => (await storage.listCards(deck.id)).length).toBe(1)
    await expect.element(screen.getByText('deck page')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/features/cards/CardEditorPage.browser.test.tsx`
Expected: FAIL — cannot find module `./CardEditorPage`.

- [ ] **Step 3: Implement `CardEditorPage`**

```tsx
// src/features/cards/CardEditorPage.tsx
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { RichMarkdownEditor } from './RichMarkdownEditor'
import { loadAssetUrl } from './assetUrl'

/** Full-screen create/edit card screen. Route params: deckId, optional cardId. */
export function CardEditorPage() {
  const { deckId, cardId } = useParams()
  const storage = useStorage()
  const navigate = useNavigate()
  const editing = Boolean(cardId)

  const [front, setFront] = useState('')
  const [back, setBack] = useState('')

  useEffect(() => {
    if (!cardId) return
    let active = true
    storage.getCard(cardId).then((card) => {
      if (active && card) {
        setFront(card.front)
        setBack(card.back)
      }
    })
    return () => {
      active = false
    }
  }, [cardId, storage])

  const ingestImage = async (file: File) => {
    const bytes = new Uint8Array(await file.arrayBuffer())
    const asset = await storage.putAsset(bytes, file.type)
    return { hash: asset.hash, mime: asset.mime }
  }
  const resolveAsset = (hash: string) => loadAssetUrl(storage, hash)

  function back2deck() {
    navigate(`/decks/${deckId}`)
  }

  async function save() {
    if (!front.trim() || !deckId) return
    if (editing && cardId) await storage.updateCard(cardId, { front, back })
    else await storage.createCard(deckId, front, back)
    await storage.sweepOrphanAssets()
    back2deck()
  }

  async function remove() {
    if (!cardId) return
    await storage.deleteCard(cardId)
    await storage.sweepOrphanAssets()
    back2deck()
  }

  const actions = (
    <>
      <button className="btn btn-primary" onClick={save} disabled={!front.trim()}>
        Save
      </button>
      <button className="btn btn-ghost" onClick={back2deck}>
        Cancel
      </button>
      {editing && (
        <button className="btn btn-ghost btn-danger btn-delete" onClick={remove}>
          Delete
        </button>
      )}
    </>
  )

  return (
    <>
      <PageHeader title={editing ? 'Edit card' : 'New card'} actions={actions} />
      <div className="page-body card-editor">
        <div className="editor-field">
          <label className="field-label">Front</label>
          <RichMarkdownEditor
            value={front}
            onChange={setFront}
            placeholder="Front (markdown)…"
            ariaLabel="Front"
            resolveAsset={resolveAsset}
            ingestImage={ingestImage}
          />
        </div>
        <div className="editor-field">
          <label className="field-label">Back</label>
          <RichMarkdownEditor
            value={back}
            onChange={setBack}
            placeholder="Back (markdown)…"
            ariaLabel="Back"
            resolveAsset={resolveAsset}
            ingestImage={ingestImage}
          />
        </div>
      </div>
    </>
  )
}
```

Add routes in `src/app/routes.tsx` (import `CardEditorPage`, add two children under the `Layout` route, before or after `decks/:deckId`):

```tsx
import { CardEditorPage } from '../features/cards/CardEditorPage'
```

```tsx
      { path: 'decks/:deckId/cards/new', element: <CardEditorPage /> },
      { path: 'decks/:deckId/cards/:cardId/edit', element: <CardEditorPage /> },
```

Update `src/features/cards/DeckDetailPage.tsx`:
- Remove `import { CardEditorModal } from './CardEditorModal'`, the `useState` for `editing`, and the `{editing && (<CardEditorModal … />)}` block.
- Add `import { useNavigate } from 'react-router-dom'` (extend the existing react-router import) and `const navigate = useNavigate()`.
- Change the two `onClick={() => setEditing({})}` to `onClick={() => navigate(\`/decks/${deckId}/cards/new\`)}`.
- Change the card row `onClick={() => setEditing({ cardId: card.id })}` to `onClick={() => navigate(\`/decks/${deckId}/cards/${card.id}/edit\`)}`.

Add editor-page layout CSS to `src/ui/styles.css`:

```css
.card-editor { display: flex; flex-direction: column; gap: 18px; }
.card-editor .editor-field { display: flex; flex-direction: column; gap: 6px; }
```

- [ ] **Step 4: Delete the modal and its orphaned CSS**

```bash
git rm src/features/cards/CardEditorModal.tsx
grep -rn "modal" src/ --include=*.tsx
```

Expected: no remaining references to `CardEditorModal` or the `modal-backdrop/modal/modal-header/modal-body/modal-footer/modal-field` classes in `.tsx`. Remove those now-orphaned `.modal*` rule blocks from `src/ui/styles.css` (only the ones no other component uses — confirm via the grep).

- [ ] **Step 5: Run tests + typecheck**

Run: `npx vitest run src/features/cards/CardEditorPage.browser.test.tsx && npm run typecheck`
Then full suite: `npm test`
Expected: PASS; clean.

- [ ] **Step 6: Commit**

```bash
git add src/features/cards/CardEditorPage.tsx src/app/routes.tsx src/features/cards/DeckDetailPage.tsx src/ui/styles.css src/features/cards/CardEditorModal.tsx
git commit -m "feat(cards): full-screen card editor route, remove modal"
```

---

## Final verification

- [ ] `npm test` — all unit + browser suites green.
- [ ] `npm run typecheck` — clean.
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` — Rust git tests green.
- [ ] Manual (desktop): `npm run app:dev` → add a card, paste an image, save, reopen — image renders; review shows the image; with a git remote configured, the same image appears after syncing a second clone.

## Notes for the implementer

- **Test isolation:** every Dexie-backed test uses a uniquely-named `RemDB('rem-…')` and `Dexie.delete` in `beforeEach`, matching `DexieStorage.test.ts`.
- **Browser vs unit:** files ending `.browser.test.tsx` run in real chromium (needed for TipTap interaction, object URLs, file uploads); everything else is jsdom.
- **Heading toggle markdown:** `tiptap-markdown` serializes images as `![alt](asset:<hash>)`; if a future change makes images inline, revisit `assetRefs` (it already matches anywhere in the string, so no change expected).
- **Known gap closed:** asset sync is included (Tasks 8–11), so images propagate across machines.
