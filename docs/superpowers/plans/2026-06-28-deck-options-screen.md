# Deck Options Screen + DeckSettings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the per-deck "Deck options" screen from `rem.dc.html` and the `DeckSettings` model behind it — persisting every setting now, with queue enforcement deferred to sub-project #3.

**Architecture:** `Deck` gains `updatedAt`, `color`, and a `settings: DeckSettings` object. A new `Storage.updateDeck(id, patch)` persists edits and stamps `updatedAt`; a Dexie v6 migration backfills existing decks; backup/snapshot round-trip the new fields; `merge.ts` switches deck reconciliation to newest-`updatedAt`-wins. A new full-screen route `/decks/:deckId/options` renders the settings UI from three reusable primitives (`Stepper`, `SegToggle`, `Toggle`).

**Tech Stack:** React 19 + react-router-dom 7, Dexie 4 (IndexedDB) via `dexie-react-hooks` `useLiveQuery`, Vitest 4 (two projects: `unit`/jsdom and `browser`/playwright), Tauri 2 (FSRS over IPC — untouched here).

## Global Constraints

- **FSRS-only.** `SchedulerKind = 'fsrs'`; no SM-2 toggle. The General section shows a static "FSRS" line.
- **UI + storage now; enforcement in #3.** #1 persists every setting and renders every control. Learning/relearning steps, daily caps, insertion order, leech, burying, and live threading of `desiredRetention`/`maximumInterval` into the `fsrs_next_states` command are **not** wired to behaviour here. Custom study renders **inert** (Start disabled).
- **`DEFAULT_DECK_SETTINGS`** (verbatim): `newPerDay 20, maxReviews 200, learnSteps '1m 10m', graduatingInterval 1, easyInterval 4, insertionOrder 'sequential', relearnSteps '10m', minimumInterval 1, leechThreshold 8, leechAction 'suspend', buryRelated true, showTimer false, desiredRetention 0.9, maximumInterval 36500`.
- **Timestamps** are epoch milliseconds (`number`).
- **Tests:** TDD — failing test first. Run a single file with `npx vitest run <path>`; typecheck with `npm run typecheck`. Browser specs end in `.browser.test.tsx`, unit specs in `.test.ts(x)`.
- **Commits:** end every commit message with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`. Work happens on branch `feat/deck-options`.

## File Structure

**Created:**
- `src/ui/Stepper.tsx` — `− value +` numeric stepper (used ~7×).
- `src/ui/SegToggle.tsx` — 2-option segmented control (used 2×).
- `src/ui/Toggle.tsx` — on/off switch (used 2×).
- `src/features/decks/DeckSettingsPage.tsx` — the Deck options screen + `parseSteps` helper.
- Test files alongside each (`*.test.tsx` for primitives, `*.browser.test.tsx` for the page, `deckSettings.test.ts` for `parseSteps`).

**Modified:**
- `src/domain/models.ts` — `Deck` fields, `DeckSettings`, `DEFAULT_DECK_SETTINGS`.
- `src/data/Storage.ts` — `DeckPatch` + `updateDeck` on the port.
- `src/data/dexie/DexieStorage.ts` — `createDeck` defaults, `updateDeck`, `importDecks` defaults.
- `src/data/dexie/db.ts` — v6 migration.
- `src/data/backup.ts` — `DeckBackup` fields + parse defaults + collect.
- `src/data/sync/snapshot.ts` — `DeckRecord` fields + deserialize normalize.
- `src/data/sync/merge.ts` — deck last-write-wins.
- `src/app/routes.tsx` — `decks/:deckId/options` route.
- `src/features/cards/DeckDetailPage.tsx` — Options button + `deck.color`.
- `src/ui/Sidebar.tsx` — `deck.color` fallback.
- `src/ui/deckColor.ts` — export `DECK_PALETTE`.
- `src/ui/styles.css` — styles for the page + primitives.

---

## Task 1: DeckSettings model + storage + v6 migration

Adds the data model and persistence. Because `Deck` gains **required** fields, this task also updates the two existing `Deck` literals (`createDeck`, `backup.test.ts`) so the build stays green.

**Files:**
- Modify: `src/domain/models.ts`
- Modify: `src/data/Storage.ts`
- Modify: `src/data/dexie/DexieStorage.ts`
- Modify: `src/data/dexie/db.ts`
- Modify: `src/data/backup.test.ts:23` (fixture only — keep build green)
- Test: `src/data/dexie/DexieStorage.test.ts`, `src/data/dexie/migration.test.ts`

**Interfaces:**
- Produces: `interface DeckSettings`, `const DEFAULT_DECK_SETTINGS: DeckSettings`, `type InsertionOrder = 'sequential' | 'random'`, `type LeechAction = 'tag' | 'suspend'`; `Deck` now has `updatedAt: number`, `color: string`, `settings: DeckSettings`; `interface DeckPatch { name?: string; color?: string; settings?: DeckSettings }`; `Storage.updateDeck(id: ID, patch: DeckPatch): Promise<void>`.

- [ ] **Step 1: Add the model.** In `src/domain/models.ts`, extend `Deck` and append the new types/default:

```ts
export interface Deck {
  id: ID
  name: string
  createdAt: number
  updatedAt: number
  color: string
  schedulerKind: SchedulerKind
  settings: DeckSettings
}

export type InsertionOrder = 'sequential' | 'random'
export type LeechAction = 'tag' | 'suspend'

/** Per-deck options. Persisted by sub-project #1; the review queue starts
 *  honouring steps/caps/order/leech/burying in #3. */
export interface DeckSettings {
  newPerDay: number
  maxReviews: number
  learnSteps: string
  graduatingInterval: number
  easyInterval: number
  insertionOrder: InsertionOrder
  relearnSteps: string
  minimumInterval: number
  leechThreshold: number
  leechAction: LeechAction
  buryRelated: boolean
  showTimer: boolean
  desiredRetention: number
  maximumInterval: number
}

export const DEFAULT_DECK_SETTINGS: DeckSettings = {
  newPerDay: 20,
  maxReviews: 200,
  learnSteps: '1m 10m',
  graduatingInterval: 1,
  easyInterval: 4,
  insertionOrder: 'sequential',
  relearnSteps: '10m',
  minimumInterval: 1,
  leechThreshold: 8,
  leechAction: 'suspend',
  buryRelated: true,
  showTimer: false,
  desiredRetention: 0.9,
  maximumInterval: 36500,
}
```

- [ ] **Step 2: Add `updateDeck` to the port.** In `src/data/Storage.ts`, add the patch type and method. Add `Deck` is already imported; add `DeckSettings` to the model import:

```ts
import type { Asset, Card, Deck, DeckSettings, ID, SchedulerKind, SchedulingState } from '../domain/models'

export interface DeckPatch {
  name?: string
  color?: string
  settings?: DeckSettings
}
```

In the `Storage` interface, directly under `deleteDeck(id: ID): Promise<void>`:

```ts
  updateDeck(id: ID, patch: DeckPatch): Promise<void>
```

- [ ] **Step 3: Write the failing storage tests.** In `src/data/dexie/DexieStorage.test.ts`, inside `describe('decks', ...)`, add:

```ts
  it('seeds a new deck with defaults: updatedAt, color, default settings', async () => {
    const before = Date.now()
    const deck = await storage.createDeck('Spanish')
    expect(deck.updatedAt).toBeGreaterThanOrEqual(before)
    expect(deck.color).toBeTruthy()
    expect(deck.settings).toEqual(DEFAULT_DECK_SETTINGS)
  })

  it('updateDeck patches fields and bumps updatedAt', async () => {
    const deck = await storage.createDeck('Spanish')
    const next = { ...DEFAULT_DECK_SETTINGS, newPerDay: 35 }
    await storage.updateDeck(deck.id, { name: 'Español', color: '#2fa86b', settings: next })

    const updated = await storage.getDeck(deck.id)
    expect(updated?.name).toBe('Español')
    expect(updated?.color).toBe('#2fa86b')
    expect(updated?.settings.newPerDay).toBe(35)
    expect(updated!.updatedAt).toBeGreaterThanOrEqual(deck.updatedAt)
  })
```

Add the default import at the top of the test file:

```ts
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
```

- [ ] **Step 4: Run them — expect FAIL.**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — `createDeck` returns no `settings`; `updateDeck` is not a function.

- [ ] **Step 5: Implement in `DexieStorage`.** In `src/data/dexie/DexieStorage.ts`, update imports and `createDeck`, and add `updateDeck`:

```ts
import type { Asset, Card, Deck, ID, SchedulerKind } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'
import type { CardPatch, DeckPatch, ImportResult, Storage } from '../Storage'
```

```ts
  async createDeck(name: string, kind: SchedulerKind = 'fsrs'): Promise<Deck> {
    const now = Date.now()
    const id = crypto.randomUUID()
    const deck: Deck = {
      id,
      name: name.trim(),
      createdAt: now,
      updatedAt: now,
      color: deckColor(id),
      schedulerKind: kind,
      settings: { ...DEFAULT_DECK_SETTINGS },
    }
    await this.db.decks.add(deck)
    return deck
  }

  async updateDeck(id: ID, patch: DeckPatch): Promise<void> {
    await this.db.decks.update(id, { ...patch, updatedAt: Date.now() })
  }
```

- [ ] **Step 6: Run storage tests — expect PASS.**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: PASS.

- [ ] **Step 7: Write the failing migration test.** In `src/data/dexie/migration.test.ts`, add a new `describe` block (imports `DEFAULT_DECK_SETTINGS` at top: `import { DEFAULT_DECK_SETTINGS } from '../../domain/models'`):

```ts
describe('deck settings migration (v6)', () => {
  it('backfills updatedAt, color and default settings on pre-v6 decks', async () => {
    const v5 = new Dexie(NAME)
    v5.version(5).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v5.open()
    await v5.table('decks').add({ id: 'd1', name: 'Old', createdAt: 1234, schedulerKind: 'fsrs' })
    v5.close()

    const db = new RemDB(NAME)
    const deck = await db.decks.get('d1')
    expect(deck?.updatedAt).toBe(1234)
    expect(deck?.color).toBeTruthy()
    expect(deck?.settings).toEqual(DEFAULT_DECK_SETTINGS)
    db.close()
  })
})
```

- [ ] **Step 8: Run it — expect FAIL.**

Run: `npx vitest run src/data/dexie/migration.test.ts`
Expected: FAIL — `updatedAt`/`color`/`settings` are `undefined`.

- [ ] **Step 9: Add the v6 migration.** In `src/data/dexie/db.ts`, add imports and append a v6 version after v5:

```ts
import { getScheduler } from '../../domain/scheduler'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'
```

```ts
    // v6: per-deck settings. Backfill updatedAt (= createdAt), a stable color,
    // and default settings on decks created before the Deck options screen.
    this.version(6)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
      })
      .upgrade(async (tx) => {
        await tx.table('decks').toCollection().modify((d) => {
          if (d.updatedAt === undefined) d.updatedAt = d.createdAt
          if (!d.color) d.color = deckColor(d.id)
          if (!d.settings) d.settings = { ...DEFAULT_DECK_SETTINGS }
        })
      })
```

- [ ] **Step 10: Run migration test — expect PASS.**

Run: `npx vitest run src/data/dexie/migration.test.ts`
Expected: PASS.

- [ ] **Step 11: Fix the build-breaking fixture.** Adding required `Deck` fields breaks the `deckA` literal in `src/data/backup.test.ts:23` at typecheck. Update **only that line** so it satisfies the `Deck` type, and add the import:

```ts
const deckA: Deck = { id: 'a', name: 'Spanish', createdAt: 10, updatedAt: 10, color: '#7e6cff', schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }
```

Add to that file's imports: `import { DEFAULT_DECK_SETTINGS } from '../domain/models'`.

Do **not** touch the `collectBackup` "collects selected decks" equality assertion here. `collectBackup` is unchanged in this task and still emits only `name/createdAt/schedulerKind/cards`, so the existing assertion keeps passing. Task 2 updates both `collectBackup` and that assertion together.

- [ ] **Step 12: Typecheck + full unit run — expect PASS.**

Run: `npm run typecheck && npx vitest run --project unit`
Expected: PASS, no type errors.

- [ ] **Step 13: Commit.**

```bash
git add src/domain/models.ts src/data/Storage.ts src/data/dexie/DexieStorage.ts src/data/dexie/db.ts src/data/dexie/DexieStorage.test.ts src/data/dexie/migration.test.ts src/data/backup.test.ts
git commit -m "feat(decks): DeckSettings model, updateDeck, and v6 migration

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: Backups carry color + settings

Round-trips the new deck fields through export/import, defaulting them for older backups.

**Files:**
- Modify: `src/data/backup.ts`
- Modify: `src/data/dexie/DexieStorage.ts` (`importDecks`)
- Test: `src/data/backup.test.ts`

**Interfaces:**
- Consumes: `DeckSettings`, `DEFAULT_DECK_SETTINGS` (Task 1).
- Produces: `DeckBackup` gains `color?: string` and `settings: DeckSettings`; `parseBackup` defaults both; `collectBackup` emits both; `importDecks` writes `color`/`settings`/`updatedAt`.

- [ ] **Step 1: Write the failing tests.** In `src/data/backup.test.ts` add (the file already imports what's needed except confirm `DEFAULT_DECK_SETTINGS` is imported from Task 1):

```ts
describe('settings round-trip', () => {
  it('preserves custom settings through serialize -> parse', () => {
    const custom = { ...DEFAULT_DECK_SETTINGS, newPerDay: 50, leechAction: 'tag' as const }
    const json = serializeBackup(
      [{ name: 'D', createdAt: 1, schedulerKind: 'fsrs', color: '#2fa86b', settings: custom, cards: [] }],
      NOW,
    )
    const parsed = parseBackup(json, NOW)
    expect(parsed[0].settings).toEqual(custom)
    expect(parsed[0].color).toBe('#2fa86b')
  })

  it('defaults settings to DEFAULT_DECK_SETTINGS for an old backup without them', () => {
    const oldFile = JSON.stringify({
      format: 'rem-backup',
      version: 1,
      exportedAt: NOW,
      decks: [{ name: 'Legacy', createdAt: 1, cards: [] }],
    })
    const parsed = parseBackup(oldFile, NOW)
    expect(parsed[0].settings).toEqual(DEFAULT_DECK_SETTINGS)
    expect(parsed[0].color).toBeUndefined()
  })
})
```

- [ ] **Step 2: Run — expect FAIL.**

Run: `npx vitest run src/data/backup.test.ts`
Expected: FAIL — `parsed[0].settings` is `undefined`.

- [ ] **Step 3: Implement in `backup.ts`.** Add `DeckSettings`/`DEFAULT_DECK_SETTINGS` to the model import, extend `DeckBackup`, and emit/parse the fields:

```ts
import type { ID, SchedulerKind, SchedulingState, DeckSettings } from '../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../domain/models'
```

```ts
export interface DeckBackup {
  name: string
  createdAt: number
  schedulerKind: SchedulerKind
  color?: string
  settings: DeckSettings
  cards: CardBackup[]
}
```

In `collectBackup`, the `out.push({...})` gains two fields:

```ts
    out.push({
      name: deck.name,
      createdAt: deck.createdAt,
      schedulerKind: deck.schedulerKind,
      color: deck.color,
      settings: deck.settings,
      cards: cards.map((c) => ({
        front: c.front,
        back: c.back,
        createdAt: c.createdAt,
        updatedAt: c.updatedAt,
        scheduling: c.scheduling,
      })),
    })
```

In `parseDeck`, return `color`/`settings` with defaults:

```ts
  return {
    name: raw.name,
    createdAt: raw.createdAt,
    schedulerKind: 'fsrs',
    color: typeof raw.color === 'string' ? raw.color : undefined,
    settings: { ...DEFAULT_DECK_SETTINGS, ...(isObject(raw.settings) ? raw.settings : {}) },
    cards: raw.cards.map((c) => parseCard(c, now)),
  }
```

Now `collectBackup` emits `color`/`settings`, so the existing "collects selected decks" assertion in `backup.test.ts` no longer matches. Update its expected object to include them (`deckA.color` is `'#7e6cff'` from Task 1):

```ts
    expect(out).toEqual([
      {
        name: 'Spanish',
        createdAt: 10,
        schedulerKind: 'fsrs',
        color: '#7e6cff',
        settings: DEFAULT_DECK_SETTINGS,
        cards: [{ front: 'hola', back: 'hello', createdAt: 11, updatedAt: 12, scheduling: sched }],
      },
    ])
```

- [ ] **Step 4: Run — expect PASS.**

Run: `npx vitest run src/data/backup.test.ts`
Expected: PASS.

- [ ] **Step 5: Failing import test.** In `src/data/dexie/DexieStorage.test.ts`, inside the `describe` that covers `importDecks` (search for `importDecks`; if none, add a new `describe('importDecks', ...)`), add:

```ts
  it('imports decks with settings, a color, and a fresh updatedAt', async () => {
    const before = Date.now()
    await storage.importDecks([
      { name: 'Imported', createdAt: 5, schedulerKind: 'fsrs', settings: { ...DEFAULT_DECK_SETTINGS, newPerDay: 7 }, cards: [] },
    ])
    const deck = (await storage.listDecks()).find((d) => d.name === 'Imported')
    expect(deck?.settings.newPerDay).toBe(7)
    expect(deck?.color).toBeTruthy()
    expect(deck!.updatedAt).toBeGreaterThanOrEqual(before)
  })
```

- [ ] **Step 6: Run — expect FAIL.**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — imported deck lacks `settings`/`color`/`updatedAt`.

- [ ] **Step 7: Implement in `importDecks`.** In `src/data/dexie/DexieStorage.ts`, the deck insert inside `importDecks` becomes:

```ts
      for (const d of decks) {
        const deckId = crypto.randomUUID()
        await this.db.decks.add({
          id: deckId,
          name: d.name,
          createdAt: d.createdAt,
          updatedAt: Date.now(),
          color: d.color ?? deckColor(deckId),
          schedulerKind: d.schedulerKind,
          settings: d.settings,
        })
```

(`deckColor` is already imported from Task 1.)

- [ ] **Step 8: Run + typecheck — expect PASS.**

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts && npm run typecheck`
Expected: PASS.

- [ ] **Step 9: Commit.**

```bash
git add src/data/backup.ts src/data/backup.test.ts src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat(backup): round-trip deck color and settings

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: Sync — snapshot fields + deck last-write-wins

`DeckRecord` gains the fields; `deserializeSnapshot` defaults them for older snapshots; `merge` reconciles decks by newest `updatedAt` so edits propagate.

**Files:**
- Modify: `src/data/sync/snapshot.ts`
- Modify: `src/data/sync/merge.ts`
- Modify: `src/data/dexie/DexieStorage.ts` (revert the Task 1 `applyMerge` bridge)
- Test: `src/data/sync/snapshot.test.ts`, `src/data/sync/merge.test.ts`, `src/data/dexie/DexieStorage.test.ts`

**Interfaces:**
- Consumes: `DeckSettings`, `DEFAULT_DECK_SETTINGS` (Task 1).
- Produces: `DeckRecord` gains `updatedAt: number`, `color: string`, `settings: DeckSettings`; `merge` deck rule = newest `updatedAt` wins.

**Background (read first):** Task 1 made `Deck` fields required while `DeckRecord` (this task) still lacked them, so Task 1 added a bridge to `DexieStorage.applyMerge`: it `bulkGet`s each existing deck and **forces the local** `updatedAt/color/settings` onto the incoming record (`prev?.updatedAt ?? d.createdAt`, etc.). That was correct only as a stopgap. Once this task adds the fields to `DeckRecord` and `merge` produces the LWW-winning full record, that bridge becomes a **bug** — it would discard a synced rename/recolor/settings change. Step 8 reverts it.

- [ ] **Step 1: Failing snapshot test.** In `src/data/sync/snapshot.test.ts`, update the `sample` deck literal to include the new fields and add a normalize test. Replace the `decks:` line of `sample`:

```ts
  decks: [{ id: 'd1', name: 'Spanish', createdAt: 1, updatedAt: 1, color: '#7e6cff', schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }],
```

Add the import and a new test:

```ts
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
```

```ts
  it('normalizes a deck file missing the v6 fields to defaults', () => {
    const files = {
      'rem.json': JSON.stringify({ format: 'rem-sync', version: 1 }),
      'decks/d1.json': JSON.stringify({ deck: { id: 'd1', name: 'Old', createdAt: 7, schedulerKind: 'fsrs' }, cards: [] }),
      'tombstones.json': '[]',
    }
    const snap = deserializeSnapshot(files)
    expect(snap.decks[0].updatedAt).toBe(7)
    expect(snap.decks[0].color).toBeTruthy()
    expect(snap.decks[0].settings).toEqual(DEFAULT_DECK_SETTINGS)
  })
```

- [ ] **Step 2: Run — expect FAIL.**

Run: `npx vitest run src/data/sync/snapshot.test.ts`
Expected: FAIL — normalized fields are `undefined`; type error on `sample` if not yet updated.

- [ ] **Step 3: Implement in `snapshot.ts`.** Add imports, extend `DeckRecord`, normalize on deserialize:

```ts
import type { SchedulerKind, SchedulingState, Tombstone, DeckSettings } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'
```

```ts
export interface DeckRecord {
  id: string
  name: string
  createdAt: number
  updatedAt: number
  color: string
  schedulerKind: SchedulerKind
  settings: DeckSettings
}
```

Add a normalizer and use it where deck files are read:

```ts
function normalizeDeck(d: DeckRecord): DeckRecord {
  return {
    ...d,
    updatedAt: d.updatedAt ?? d.createdAt,
    color: d.color ?? deckColor(d.id),
    settings: d.settings ?? DEFAULT_DECK_SETTINGS,
  }
}
```

In `deserializeSnapshot`, change the deck-file branch:

```ts
    if (path.startsWith('decks/') && path.endsWith('.json')) {
      const { deck, cards: deckCards } = JSON.parse(content) as DeckFile
      decks.push(normalizeDeck(deck))
      for (const c of deckCards) cards.push(c)
    }
```

- [ ] **Step 4: Run — expect PASS.**

Run: `npx vitest run src/data/sync/snapshot.test.ts`
Expected: PASS.

- [ ] **Step 5: Failing merge test.** In `src/data/sync/merge.test.ts`, update the shared `deck` fixture and add a LWW test. Replace line 5:

```ts
const deck: DeckRecord = { id: 'd1', name: 'D', createdAt: 1, updatedAt: 1, color: '#7e6cff', schedulerKind: 'fsrs', settings: DEFAULT_DECK_SETTINGS }
```

Add the import and a new test inside `describe('merge', ...)`:

```ts
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
```

```ts
  it('keeps the newer deck edit (deck LWW by updatedAt)', () => {
    const oldDeck: DeckRecord = { ...deck, name: 'old', updatedAt: 10 }
    const newDeck: DeckRecord = { ...deck, name: 'new', updatedAt: 20 }
    const { merged } = merge(snap({ decks: [oldDeck] }), snap({ decks: [newDeck] }))
    expect(merged.decks).toHaveLength(1)
    expect(merged.decks[0].name).toBe('new')
  })
```

- [ ] **Step 6: Run — expect FAIL.**

Run: `npx vitest run src/data/sync/merge.test.ts`
Expected: FAIL — current union keeps local (`old`) instead of newest (`new`).

- [ ] **Step 7: Implement deck LWW in `merge.ts`.** Replace the deck-union block (lines ~34–38) with newest-wins, keeping the tombstone-drop loop below it:

```ts
  // Decks: union by id, newest updatedAt wins (deck name/color/settings are
  // editable). Drop if a deck tombstone is at/after the deck's creation.
  const deckById = new Map<string, DeckRecord>()
  for (const d of [...remote.decks, ...local.decks]) {
    const prev = deckById.get(d.id)
    if (!prev || d.updatedAt > prev.updatedAt) deckById.set(d.id, d)
  }
```

- [ ] **Step 8: Revert the Task 1 `applyMerge` bridge (now that `DeckRecord` is complete).** Write a failing test first, then revert.

In `src/data/dexie/DexieStorage.test.ts`, add (the file already imports `DEFAULT_DECK_SETTINGS` from Task 1):

```ts
  it('applyMerge applies a synced deck color/settings change', async () => {
    const deck = await storage.createDeck('Spanish')
    const incoming = {
      id: deck.id,
      name: 'Spanish',
      createdAt: deck.createdAt,
      updatedAt: deck.updatedAt + 1000,
      color: '#e8638c',
      schedulerKind: 'fsrs' as const,
      settings: { ...DEFAULT_DECK_SETTINGS, newPerDay: 99 },
    }
    await storage.applyMerge({
      upsertDecks: [incoming], upsertCards: [], deleteDeckIds: [], deleteCardIds: [],
      tombstones: [], upsertAssets: [], deleteAssetHashes: [],
    })
    const after = await storage.getDeck(deck.id)
    expect(after?.color).toBe('#e8638c')
    expect(after?.settings.newPerDay).toBe(99)
  })
```

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — the Task 1 bridge forces the local color/settings, so `color` stays the seeded value and `newPerDay` stays 20.

Now revert the deck branch of `applyMerge` in `src/data/dexie/DexieStorage.ts` to the plain upsert (the merged `DeckRecord` is authoritative now). Replace the entire `if (ops.upsertDecks.length) { ... }` block — the one that does `bulkGet`/`existingById`/the field-preserving `map` — with:

```ts
        if (ops.upsertDecks.length) await this.db.decks.bulkPut(ops.upsertDecks)
```

`deckColor` and `DEFAULT_DECK_SETTINGS` remain imported in this file (still used by `createDeck`), so no import changes — confirm with the typecheck below.

Run: `npx vitest run src/data/dexie/DexieStorage.test.ts`
Expected: PASS.

- [ ] **Step 9: Run merge + full unit suite — expect PASS.**

Run: `npx vitest run src/data/sync/merge.test.ts && npm run typecheck && npx vitest run --project unit`
Expected: PASS. In particular `src/data/sync/GitSyncService.test.ts` must stay green: with `DeckRecord` now field-complete, the export→serialize→deserialize→merge→applyMerge round-trip preserves every deck field, so the sync is naturally idempotent without the Task 1 bridge. If GitSyncService fails here, STOP and report — do not re-add the bridge.

- [ ] **Step 10: Commit.**

```bash
git add src/data/sync/snapshot.ts src/data/sync/snapshot.test.ts src/data/sync/merge.ts src/data/sync/merge.test.ts src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat(sync): deck color/settings in snapshot + deck last-write-wins merge

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: UI primitives — Stepper, SegToggle, Toggle

Three reusable controls the screen composes. Each gets a focused browser test (they render interactive DOM).

**Files:**
- Create: `src/ui/Stepper.tsx`, `src/ui/Stepper.browser.test.tsx`
- Create: `src/ui/SegToggle.tsx`, `src/ui/SegToggle.browser.test.tsx`
- Create: `src/ui/Toggle.tsx`, `src/ui/Toggle.browser.test.tsx`
- Modify: `src/ui/styles.css`

**Interfaces:**
- Produces:
  - `Stepper({ value, onChange, label, step?, min?, max?, format? })` — `value: number`, `onChange: (next: number) => void`, `label: string`, `step?: number` (default 1), `min?/max?: number`, `format?: (v: number) => string`. Buttons are aria-labelled `Decrease ${label}` / `Increase ${label}`; clamps to `[min, max]`.
  - `SegToggle({ value, onChange, options })` — `options: { value: T; label: string }[]`, `value: T`, `onChange: (next: T) => void`. Active button has `aria-pressed`.
  - `Toggle({ checked, onChange, label })` — `role="switch"`, `aria-checked`, `aria-label={label}`.

- [ ] **Step 1: Stepper test (failing).** `src/ui/Stepper.browser.test.tsx`:

```tsx
import { test, expect, vi } from 'vitest'
import { page } from 'vitest/browser'
import { render } from 'vitest-browser-react'
import { Stepper } from './Stepper'

test('formats the value and increases by step, clamped to max', async () => {
  const onChange = vi.fn()
  render(<Stepper value={4} onChange={onChange} label="Easy interval" step={1} min={1} max={4} format={(v) => `${v}d`} />)

  await expect.element(page.getByText('4d')).toBeVisible()
  await page.getByLabelText('Increase Easy interval').click()
  expect(onChange).toHaveBeenCalledWith(4) // clamped at max

  await page.getByLabelText('Decrease Easy interval').click()
  expect(onChange).toHaveBeenCalledWith(3)
})
```

- [ ] **Step 2: Run — expect FAIL.**

Run: `npx vitest run src/ui/Stepper.browser.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `Stepper.tsx`.**

```tsx
/** Numeric stepper: − value +, with optional clamp and display formatter. */
export function Stepper({
  value,
  onChange,
  label,
  step = 1,
  min,
  max,
  format,
}: {
  value: number
  onChange: (next: number) => void
  label: string
  step?: number
  min?: number
  max?: number
  format?: (v: number) => string
}) {
  const clamp = (v: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v))
  return (
    <div className="stepper">
      <button type="button" className="stepper-btn" aria-label={`Decrease ${label}`} onClick={() => onChange(clamp(value - step))}>
        −
      </button>
      <span className="stepper-val">{format ? format(value) : value}</span>
      <button type="button" className="stepper-btn" aria-label={`Increase ${label}`} onClick={() => onChange(clamp(value + step))}>
        +
      </button>
    </div>
  )
}
```

- [ ] **Step 4: Run — expect PASS.**

Run: `npx vitest run src/ui/Stepper.browser.test.tsx`
Expected: PASS.

- [ ] **Step 5: SegToggle test (failing).** `src/ui/SegToggle.browser.test.tsx`:

```tsx
import { test, expect, vi } from 'vitest'
import { page } from 'vitest/browser'
import { render } from 'vitest-browser-react'
import { SegToggle } from './SegToggle'

test('marks the active option and reports the other on click', async () => {
  const onChange = vi.fn()
  render(
    <SegToggle
      value="sequential"
      onChange={onChange}
      options={[
        { value: 'sequential', label: 'SEQ' },
        { value: 'random', label: 'RANDOM' },
      ]}
    />,
  )
  await expect.element(page.getByRole('button', { name: 'SEQ' })).toHaveAttribute('aria-pressed', 'true')
  await page.getByRole('button', { name: 'RANDOM' }).click()
  expect(onChange).toHaveBeenCalledWith('random')
})
```

- [ ] **Step 6: Run — expect FAIL.** Run: `npx vitest run src/ui/SegToggle.browser.test.tsx` → FAIL.

- [ ] **Step 7: Implement `SegToggle.tsx`.**

```tsx
/** Two-or-more option segmented control. */
export function SegToggle<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T
  onChange: (next: T) => void
  options: { value: T; label: string }[]
}) {
  return (
    <div className="seg" role="group">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          className={o.value === value ? 'seg-btn is-active' : 'seg-btn'}
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}
```

- [ ] **Step 8: Run — expect PASS.** Run: `npx vitest run src/ui/SegToggle.browser.test.tsx` → PASS.

- [ ] **Step 9: Toggle test (failing).** `src/ui/Toggle.browser.test.tsx`:

```tsx
import { test, expect, vi } from 'vitest'
import { page } from 'vitest/browser'
import { render } from 'vitest-browser-react'
import { Toggle } from './Toggle'

test('reflects checked state and flips on click', async () => {
  const onChange = vi.fn()
  render(<Toggle checked={false} onChange={onChange} label="Bury related new cards" />)
  const sw = page.getByRole('switch', { name: 'Bury related new cards' })
  await expect.element(sw).toHaveAttribute('aria-checked', 'false')
  await sw.click()
  expect(onChange).toHaveBeenCalledWith(true)
})
```

- [ ] **Step 10: Run — expect FAIL.** Run: `npx vitest run src/ui/Toggle.browser.test.tsx` → FAIL.

- [ ] **Step 11: Implement `Toggle.tsx`.**

```tsx
/** On/off switch. */
export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (next: boolean) => void
  label: string
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={checked ? 'toggle is-on' : 'toggle'}
      onClick={() => onChange(!checked)}
    >
      <span className="toggle-knob" />
    </button>
  )
}
```

- [ ] **Step 12: Add primitive styles.** Append to `src/ui/styles.css`:

```css
/* Deck-options primitives */
.stepper { display: flex; align-items: center; gap: 2px; background: var(--surface-inset); border: 1px solid var(--border); border-radius: 11px; padding: 3px; }
.stepper-btn { width: 30px; height: 30px; border: none; border-radius: 8px; background: transparent; color: var(--muted); font-size: 18px; cursor: pointer; display: flex; align-items: center; justify-content: center; }
.stepper-btn:hover { background: var(--surface); color: var(--text); }
.stepper-val { min-width: 56px; text-align: center; font-family: var(--font-mono); font-size: 15px; font-weight: 700; }

.seg { display: flex; gap: 4px; background: var(--surface-inset); border: 1px solid var(--border); border-radius: 11px; padding: 3px; }
.seg-btn { padding: 7px 14px; border-radius: 8px; border: none; cursor: pointer; font-family: var(--font-mono); font-size: 12px; font-weight: 700; background: transparent; color: var(--muted); }
.seg-btn.is-active { background: var(--accent); color: var(--on-accent); }

.toggle { width: 42px; height: 24px; border-radius: 999px; border: none; background: var(--border-strong); cursor: pointer; padding: 0; position: relative; flex: none; transition: background 0.2s; }
.toggle.is-on { background: var(--accent); }
.toggle-knob { position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: transform 0.2s; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3); }
.toggle.is-on .toggle-knob { transform: translateX(18px); }
```

- [ ] **Step 13: Run all three primitive specs + typecheck — expect PASS.**

Run: `npx vitest run src/ui/Stepper.browser.test.tsx src/ui/SegToggle.browser.test.tsx src/ui/Toggle.browser.test.tsx && npm run typecheck`
Expected: PASS.

- [ ] **Step 14: Commit.**

```bash
git add src/ui/Stepper.tsx src/ui/Stepper.browser.test.tsx src/ui/SegToggle.tsx src/ui/SegToggle.browser.test.tsx src/ui/Toggle.tsx src/ui/Toggle.browser.test.tsx src/ui/styles.css
git commit -m "feat(ui): Stepper, SegToggle, and Toggle primitives

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 5: Deck options page — scaffold + General section + route + entry point

Creates the screen with its header and the General card (rename, color swatches, FSRS line, desired-retention stepper), the route, the Options button, and the `deck.color` swaps.

**Files:**
- Create: `src/features/decks/DeckSettingsPage.tsx`, `src/features/decks/DeckSettingsPage.browser.test.tsx`
- Modify: `src/app/routes.tsx`
- Modify: `src/features/cards/DeckDetailPage.tsx`
- Modify: `src/ui/Sidebar.tsx`
- Modify: `src/ui/deckColor.ts`
- Modify: `src/ui/styles.css`

**Interfaces:**
- Consumes: `Stepper`, `SegToggle`, `Toggle` (Task 4); `Storage.updateDeck`, `DeckSettings`, `DEFAULT_DECK_SETTINGS` (Task 1); `DECK_PALETTE`, `deckColor`.
- Produces: `DeckSettingsPage` (default route element); the route `decks/:deckId/options`.

- [ ] **Step 1: Export the palette.** In `src/ui/deckColor.ts`, change `const DECK_PALETTE` to `export const DECK_PALETTE` (one word). Leave `deckColor` as-is.

- [ ] **Step 2: Failing page test.** `src/features/decks/DeckSettingsPage.browser.test.tsx`:

```tsx
import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckSettingsPage } from './DeckSettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('renders the General section and persists a rename on blur', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')

  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await expect.element(page.getByText('Deck options')).toBeVisible()
  const name = page.getByLabelText('Deck name')
  await name.fill('Español')
  await name.element().blur()

  await expect.poll(async () => (await storage.getDeck(deck.id))?.name).toBe('Español')
})

test('persists a color swatch and the desired-retention stepper', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByLabelText('Color #2fa86b').click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.color).toBe('#2fa86b')

  await page.getByLabelText('Increase Desired retention').click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.desiredRetention).toBe(0.91)
})
```

- [ ] **Step 3: Run — expect FAIL.** Run: `npx vitest run src/features/decks/DeckSettingsPage.browser.test.tsx` → FAIL (module not found).

- [ ] **Step 4: Create `DeckSettingsPage.tsx` (scaffold + General).** This is the file later tasks extend; it is complete as written here.

```tsx
import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import { useStorage } from '../../data/StorageContext'
import type { Storage } from '../../data/Storage'
import type { Deck, DeckSettings } from '../../domain/models'
import { PageHeader } from '../../ui/PageHeader'
import { Stepper } from '../../ui/Stepper'
import { DECK_PALETTE, deckColor } from '../../ui/deckColor'

/** Split a space-separated steps string into chip tokens. */
export function parseSteps(raw: string): string[] {
  return raw.split(/\s+/).filter(Boolean)
}

export function DeckSettingsPage() {
  const { deckId } = useParams()
  const storage = useStorage()
  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  if (!deckId || deck === undefined) return null
  // Remount on deck change so local form state re-seeds from storage.
  return <DeckSettingsForm key={deck.id} deck={deck} storage={storage} />
}

function DeckSettingsForm({ deck, storage }: { deck: Deck; storage: Storage }) {
  const navigate = useNavigate()
  const [name, setName] = useState(deck.name)
  const [color, setColor] = useState(deck.color)
  const [settings, setSettings] = useState<DeckSettings>(deck.settings)

  function pickColor(c: string) {
    setColor(c)
    void storage.updateDeck(deck.id, { color: c })
  }
  /** Update one setting and persist immediately (steppers, toggles, segmented).
   *  The `as DeckSettings` cast is required: TS can't narrow a generic
   *  computed-key spread under strict mode. */
  function set<K extends keyof DeckSettings>(key: K, value: DeckSettings[K]) {
    const next = { ...settings, [key]: value } as DeckSettings
    setSettings(next)
    void storage.updateDeck(deck.id, { settings: next })
  }

  const title = (
    <>
      <button className="back-link" aria-label="Back to deck" onClick={() => navigate(`/decks/${deck.id}`)}>
        ‹ {deck.name}
      </button>
      <span className="header-dot" style={{ background: color || deckColor(deck.id) }} />
      <span className="header-title-text">Deck options</span>
    </>
  )

  return (
    <>
      <PageHeader title={title} />
      <div className="page-body">
        <div className="deck-settings">
          {/* GENERAL */}
          <div className="ds-label">General</div>
          <div className="ds-card">
            <label className="ds-field-label" htmlFor="ds-name">Deck name</label>
            <input
              id="ds-name"
              className="ds-name-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onBlur={() => storage.updateDeck(deck.id, { name })}
            />
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Color</div>
                <div className="ds-row-sub">Shown in the sidebar and on cards.</div>
              </div>
              <div className="ds-swatches">
                {DECK_PALETTE.map((c) => (
                  <button
                    key={c}
                    type="button"
                    aria-label={`Color ${c}`}
                    aria-pressed={c === color}
                    className={c === color ? 'ds-swatch is-active' : 'ds-swatch'}
                    style={{ background: c }}
                    onClick={() => pickColor(c)}
                  />
                ))}
              </div>
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Scheduler</div>
                <div className="ds-row-sub">FSRS adapts intervals to your recall.</div>
              </div>
              <span className="algo-chip">FSRS</span>
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Desired retention</div>
                <div className="ds-row-sub">Target probability of recall at review time.</div>
              </div>
              <Stepper
                value={settings.desiredRetention}
                onChange={(v) => set('desiredRetention', Math.round(v * 100) / 100)}
                label="Desired retention"
                step={0.01}
                min={0.7}
                max={0.99}
                format={(v) => `${Math.round(v * 100)}%`}
              />
            </div>
          </div>
        </div>
      </div>
    </>
  )
}
```

Note: this file is intentionally complete-as-is for Task 5 — every local and import is used (`set` by the retention stepper, `pickColor` by the swatches). Tasks 6 and 7 add the `setLocal`/`commit` helpers, the `SegToggle`/`Toggle` imports, and the remaining sections; do **not** add them now or `noUnusedLocals`/`verbatimModuleSyntax` will fail the typecheck at Step 12.

- [ ] **Step 5: Register the route.** In `src/app/routes.tsx`, add the import and route. Import:

```tsx
import { DeckSettingsPage } from '../features/decks/DeckSettingsPage'
```

Add inside `children`, after the `decks/:deckId` route:

```tsx
      { path: 'decks/:deckId/options', element: <DeckSettingsPage /> },
```

- [ ] **Step 6: Add the General-section + header styles.** Append to `src/ui/styles.css`:

```css
/* Deck options screen */
.back-link { border: none; background: transparent; color: var(--muted); font: inherit; font-weight: 500; cursor: pointer; padding: 0; }
.back-link:hover { color: var(--text); }
.deck-settings { max-width: 680px; margin: 0 auto; }
.ds-label { font-family: var(--font-mono); font-size: 11px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--faint); margin: 0 0 12px; }
.ds-label.is-danger { color: var(--danger); }
.ds-card { background: var(--surface); border: 1px solid var(--border); border-radius: 16px; padding: 20px; margin-bottom: 30px; }
.ds-card.is-danger { border-color: color-mix(in oklch, var(--danger), transparent 70%); }
.ds-field-label { display: block; font-size: 13px; color: var(--muted); margin-bottom: 8px; }
.ds-name-input { width: 100%; padding: 12px 14px; border-radius: 10px; border: 1px solid var(--border-strong); background: var(--bg); color: var(--text); font-family: var(--font-serif); font-size: 21px; }
.ds-rule { height: 1px; background: var(--border); margin: 20px 0; }
.ds-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.ds-row-title { font-size: 15px; font-weight: 500; }
.ds-row-sub { font-size: 13px; color: var(--muted); margin-top: 3px; }
.ds-swatches { display: flex; gap: 10px; }
.ds-swatch { width: 28px; height: 28px; border-radius: 50%; border: 2px solid var(--surface); box-shadow: 0 0 0 1px var(--border-strong); cursor: pointer; padding: 0; }
.ds-swatch.is-active { box-shadow: 0 0 0 2px var(--accent); }
.ds-steps-input { width: 100%; padding: 11px 14px; border-radius: 10px; border: 1px solid var(--border-strong); background: var(--bg); color: var(--text); font-family: var(--font-mono); font-size: 14px; }
.ds-chips { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 12px; }
.ds-chip { font-family: var(--font-mono); font-size: 12px; font-weight: 700; padding: 5px 11px; border-radius: 8px; background: var(--accent-soft); color: var(--accent-text); }
.ds-chip.is-lapse { background: color-mix(in oklch, var(--hard), transparent 86%); color: var(--hard); }
.ds-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; margin-bottom: 12px; }
.ds-preset { text-align: left; cursor: pointer; background: var(--surface); border: 1px solid var(--border); border-radius: 13px; padding: 15px 16px; display: flex; flex-direction: column; gap: 4px; }
.ds-preset.is-active { border-color: var(--accent); }
.ds-preset-title { font-size: 14.5px; font-weight: 600; }
.ds-preset-sub { font-size: 12.5px; color: var(--muted); }
.ds-custom-run { display: flex; align-items: center; gap: 18px; background: var(--surface); border: 1px solid var(--accent); border-radius: 13px; padding: 16px 18px; margin-bottom: 30px; }
.ds-custom-run > .ds-row-title { flex: 1; }
```

- [ ] **Step 7: Run page test — expect PASS.**

Run: `npx vitest run src/features/decks/DeckSettingsPage.browser.test.tsx`
Expected: PASS.

- [ ] **Step 8: Failing entry-point test.** Add to `src/features/cards/DeckDetailPage.browser.test.tsx` (create the file if absent — match the imports used by `DeckListPage.browser.test.tsx`):

```tsx
import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckDetailPage } from './DeckDetailPage'
import { DeckSettingsPage } from '../decks/DeckSettingsPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('Options button opens the deck options screen', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId',
    entry: `/decks/${deck.id}`,
    element: <DeckDetailPage />,
    extraRoutes: [{ path: '/decks/:deckId/options', element: <DeckSettingsPage /> }],
  })

  await page.getByRole('button', { name: 'Options' }).click()
  await expect.element(page.getByText('Deck options')).toBeVisible()
})
```

- [ ] **Step 9: Run — expect FAIL.** Run: `npx vitest run src/features/cards/DeckDetailPage.browser.test.tsx` → FAIL (no Options button).

- [ ] **Step 10: Add the Options button + `deck.color`.** In `src/features/cards/DeckDetailPage.tsx`:

Change the header dot to use the stored color:

```tsx
      <span className="header-dot" style={{ background: deck.color ?? deckColor(deck.id) }} />
```

Replace the `actions` definition so Options always shows:

```tsx
  const actions = (
    <>
      <button className="btn btn-ghost" onClick={() => navigate(`/decks/${deckId}/options`)}>
        Options
      </button>
      {cards.length > 0 && (
        <>
          <button className="btn btn-ghost" onClick={() => navigate(`/decks/${deckId}/cards/new`)}>
            + Add card
          </button>
          {due && due > 0 ? (
            <Link to={`/decks/${deckId}/study`} className="btn btn-primary">
              Study {due}
            </Link>
          ) : (
            <span className="muted">All caught up today</span>
          )}
        </>
      )}
    </>
  )
```

- [ ] **Step 11: Swap the Sidebar dot color.** In `src/ui/Sidebar.tsx`, the deck dot becomes:

```tsx
            <span className="deck-dot" style={{ background: deck.color ?? deckColor(deck.id) }} />
```

- [ ] **Step 12: Run entry-point test + typecheck — expect PASS.**

Run: `npx vitest run src/features/cards/DeckDetailPage.browser.test.tsx && npm run typecheck`
Expected: PASS.

- [ ] **Step 13: Commit.**

```bash
git add src/features/decks/DeckSettingsPage.tsx src/features/decks/DeckSettingsPage.browser.test.tsx src/app/routes.tsx src/features/cards/DeckDetailPage.tsx src/features/cards/DeckDetailPage.browser.test.tsx src/ui/Sidebar.tsx src/ui/deckColor.ts src/ui/styles.css
git commit -m "feat(decks): deck options screen scaffold, General section, and entry point

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 6: Daily limits, New cards, and Lapses sections

Adds the bulk of the form: steppers, step-string inputs with live chips, and segmented controls — all wired to persist.

**Files:**
- Modify: `src/features/decks/DeckSettingsPage.tsx`
- Create: `src/features/decks/deckSettings.test.ts`
- Modify: `src/features/decks/DeckSettingsPage.browser.test.tsx`

**Interfaces:**
- Consumes: `Stepper`, `SegToggle` (Task 4); `set`/`setLocal`/`commit`/`parseSteps` (Task 5).

- [ ] **Step 1: `parseSteps` unit test (failing).** `src/features/decks/deckSettings.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { parseSteps } from './DeckSettingsPage'

describe('parseSteps', () => {
  it('splits a space-separated steps string into tokens', () => {
    expect(parseSteps('1m 10m 1d')).toEqual(['1m', '10m', '1d'])
  })
  it('collapses extra whitespace and drops empties', () => {
    expect(parseSteps('  10m   1d ')).toEqual(['10m', '1d'])
  })
  it('returns [] for a blank string', () => {
    expect(parseSteps('   ')).toEqual([])
  })
})
```

- [ ] **Step 2: Run — expect PASS** (`parseSteps` already exists from Task 5).

Run: `npx vitest run src/features/decks/deckSettings.test.ts`
Expected: PASS. (This locks the helper's contract; if it fails, fix `parseSteps`.)

- [ ] **Step 3: Failing browser test for the new sections.** Add to `src/features/decks/DeckSettingsPage.browser.test.tsx`:

```tsx
test('persists daily-limit, new-card, and lapse edits', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByLabelText('Increase New cards/day').click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.newPerDay).toBe(25)

  await page.getByRole('button', { name: 'RANDOM' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.insertionOrder).toBe('random')

  const learn = page.getByLabelText('Learning steps')
  await learn.fill('1m 10m 1h')
  await learn.element().blur()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.learnSteps).toBe('1m 10m 1h')

  await page.getByRole('button', { name: 'TAG' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.leechAction).toBe('tag')
})
```

- [ ] **Step 4: Run — expect FAIL.** Run: `npx vitest run src/features/decks/DeckSettingsPage.browser.test.tsx` → FAIL (controls absent).

- [ ] **Step 5: Add the helpers, the import, and the three sections.** In `DeckSettingsPage.tsx`:

(a) Add the `SegToggle` import:

```tsx
import { SegToggle } from '../../ui/SegToggle'
```

(b) Add two helpers in `DeckSettingsForm`, directly after the `set` function (used by the step-string inputs below — text fields update locally on each keystroke for live chips, then persist on blur):

```tsx
  /** Update one setting locally only (text inputs persist on blur via commit). */
  function setLocal<K extends keyof DeckSettings>(key: K, value: DeckSettings[K]) {
    setSettings((s) => ({ ...s, [key]: value }) as DeckSettings)
  }
  function commit() {
    void storage.updateDeck(deck.id, { settings })
  }
```

(c) Insert the JSX **after** the General `.ds-card` (still inside `.deck-settings`):

```tsx
          {/* DAILY LIMITS */}
          <div className="ds-label">Daily limits</div>
          <div className="ds-card">
            <div className="ds-row">
              <div>
                <div className="ds-row-title">New cards/day</div>
                <div className="ds-row-sub">Cap on new cards introduced daily.</div>
              </div>
              <Stepper value={settings.newPerDay} onChange={(v) => set('newPerDay', v)} label="New cards/day" step={5} min={0} max={9999} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Maximum reviews/day</div>
                <div className="ds-row-sub">Cap on due reviews shown each day.</div>
              </div>
              <Stepper value={settings.maxReviews} onChange={(v) => set('maxReviews', v)} label="Maximum reviews/day" step={10} min={0} max={9999} />
            </div>
          </div>

          {/* NEW CARDS */}
          <div className="ds-label">New cards</div>
          <div className="ds-card">
            <div className="ds-row">
              <div className="ds-row-title">Learning steps</div>
              <span className="ds-row-sub" style={{ fontFamily: 'var(--font-mono)' }}>e.g. 1m 10m 1d</span>
            </div>
            <div className="ds-row-sub" style={{ margin: '3px 0 12px' }}>Intervals a new card steps through before graduating. Space-separated.</div>
            <input
              className="ds-steps-input"
              aria-label="Learning steps"
              value={settings.learnSteps}
              onChange={(e) => setLocal('learnSteps', e.target.value)}
              onBlur={commit}
            />
            <div className="ds-chips">
              {parseSteps(settings.learnSteps).map((s, i) => (
                <span key={`${s}-${i}`} className="ds-chip">{s}</span>
              ))}
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Graduating interval</div>
                <div className="ds-row-sub">Days until next review after the last step.</div>
              </div>
              <Stepper value={settings.graduatingInterval} onChange={(v) => set('graduatingInterval', v)} label="Graduating interval" step={1} min={1} max={365} format={(v) => `${v}d`} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Easy interval</div>
                <div className="ds-row-sub">Days when answering a new card “Easy”.</div>
              </div>
              <Stepper value={settings.easyInterval} onChange={(v) => set('easyInterval', v)} label="Easy interval" step={1} min={1} max={365} format={(v) => `${v}d`} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Insertion order</div>
                <div className="ds-row-sub">Order new cards enter the queue.</div>
              </div>
              <SegToggle
                value={settings.insertionOrder}
                onChange={(v) => set('insertionOrder', v)}
                options={[{ value: 'sequential', label: 'SEQ' }, { value: 'random', label: 'RANDOM' }]}
              />
            </div>
          </div>

          {/* LAPSES */}
          <div className="ds-label">Lapses</div>
          <div className="ds-card">
            <div className="ds-row">
              <div className="ds-row-title">Relearning steps</div>
              <span className="ds-row-sub" style={{ fontFamily: 'var(--font-mono)' }}>e.g. 10m</span>
            </div>
            <div className="ds-row-sub" style={{ margin: '3px 0 12px' }}>Steps a lapsed card relearns through. Space-separated.</div>
            <input
              className="ds-steps-input"
              aria-label="Relearning steps"
              value={settings.relearnSteps}
              onChange={(e) => setLocal('relearnSteps', e.target.value)}
              onBlur={commit}
            />
            <div className="ds-chips">
              {parseSteps(settings.relearnSteps).map((s, i) => (
                <span key={`${s}-${i}`} className="ds-chip is-lapse">{s}</span>
              ))}
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Minimum interval</div>
                <div className="ds-row-sub">Floor for intervals after a lapse.</div>
              </div>
              <Stepper value={settings.minimumInterval} onChange={(v) => set('minimumInterval', v)} label="Minimum interval" step={1} min={1} max={365} format={(v) => `${v}d`} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Leech threshold</div>
                <div className="ds-row-sub">Lapses before a card is flagged a leech.</div>
              </div>
              <Stepper value={settings.leechThreshold} onChange={(v) => set('leechThreshold', v)} label="Leech threshold" step={1} min={1} max={99} />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Leech action</div>
                <div className="ds-row-sub">What happens when a leech is found.</div>
              </div>
              <SegToggle
                value={settings.leechAction}
                onChange={(v) => set('leechAction', v)}
                options={[{ value: 'tag', label: 'TAG' }, { value: 'suspend', label: 'SUSPEND' }]}
              />
            </div>
          </div>
```

- [ ] **Step 6: Run page tests — expect PASS.**

Run: `npx vitest run src/features/decks/DeckSettingsPage.browser.test.tsx src/features/decks/deckSettings.test.ts`
Expected: PASS.

- [ ] **Step 7: Typecheck — expect PASS.** Run: `npm run typecheck` → PASS (`setLocal`/`commit` are now used).

- [ ] **Step 8: Commit.**

```bash
git add src/features/decks/DeckSettingsPage.tsx src/features/decks/DeckSettingsPage.browser.test.tsx src/features/decks/deckSettings.test.ts
git commit -m "feat(decks): daily limits, new cards, and lapses sections

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 7: Custom study (inert), Burying & timer, and Danger zone

Completes the screen: the inert custom-study card, the two toggles, and delete-with-confirm.

**Files:**
- Modify: `src/features/decks/DeckSettingsPage.tsx`
- Modify: `src/features/decks/DeckSettingsPage.browser.test.tsx`
- Modify: `src/ui/styles.css`

**Interfaces:**
- Consumes: `Toggle` (Task 4); `Storage.deleteDeck`; `set`/`settings` (Task 5).

- [ ] **Step 1: Failing browser tests.** Add to `src/features/decks/DeckSettingsPage.browser.test.tsx`:

```tsx
test('toggles bury/timer and keeps Custom study Start inert', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
  })

  await page.getByRole('switch', { name: 'Show answer timer' }).click()
  await expect.poll(async () => (await storage.getDeck(deck.id))?.settings.showTimer).toBe(true)

  await expect.element(page.getByRole('button', { name: 'Start' })).toBeDisabled()
})

test('deletes the deck after confirm and navigates away', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Spanish')
  await renderRoute({
    storage,
    path: '/decks/:deckId/options',
    entry: `/decks/${deck.id}/options`,
    element: <DeckSettingsPage />,
    extraRoutes: [{ path: '/', element: <div>Home</div> }],
  })

  await page.getByRole('button', { name: 'Delete deck' }).click()
  await page.getByRole('button', { name: 'Confirm delete' }).click()
  await expect.poll(async () => await storage.getDeck(deck.id)).toBeUndefined()
})
```

- [ ] **Step 2: Run — expect FAIL.** Run: `npx vitest run src/features/decks/DeckSettingsPage.browser.test.tsx` → FAIL.

- [ ] **Step 3: Add imports + delete state.** In `DeckSettingsPage.tsx`, add `Toggle` to imports and a confirm-state hook in `DeckSettingsForm`:

```tsx
import { Toggle } from '../../ui/Toggle'
```

Inside `DeckSettingsForm`, after the `settings` state:

```tsx
  const [confirmDelete, setConfirmDelete] = useState(false)
```

- [ ] **Step 4: Add the three sections.** Insert after the Lapses `.ds-card`, before the closing `</div>` of `.deck-settings`:

```tsx
          {/* CUSTOM STUDY (inert — behaviour is sub-project #4) */}
          <div className="ds-label">Custom study</div>
          <div className="ds-grid">
            {[
              { title: 'Study ahead', sub: 'Review cards due later.' },
              { title: 'Increase new', sub: 'More new cards today.' },
              { title: 'Review forgotten', sub: 'Re-see recent lapses.' },
              { title: 'Preview new', sub: 'Peek at upcoming cards.' },
            ].map((p) => (
              <button key={p.title} type="button" className="ds-preset" disabled>
                <span className="ds-preset-title">{p.title}</span>
                <span className="ds-preset-sub">{p.sub}</span>
              </button>
            ))}
          </div>
          <div className="ds-custom-run">
            <div className="ds-row-title">Custom study</div>
            <Stepper value={10} onChange={() => {}} label="Custom study count" step={5} min={5} max={999} />
            <button type="button" className="btn btn-primary" disabled>Start</button>
          </div>

          {/* BURYING & TIMER */}
          <div className="ds-label">Burying &amp; timer</div>
          <div className="ds-card">
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Bury related new cards</div>
                <div className="ds-row-sub">Hold siblings until the next day.</div>
              </div>
              <Toggle checked={settings.buryRelated} onChange={(v) => set('buryRelated', v)} label="Bury related new cards" />
            </div>
            <div className="ds-rule" />
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Show answer timer</div>
                <div className="ds-row-sub">Display time spent on each card.</div>
              </div>
              <Toggle checked={settings.showTimer} onChange={(v) => set('showTimer', v)} label="Show answer timer" />
            </div>
          </div>

          {/* DANGER ZONE */}
          <div className="ds-label is-danger">Danger zone</div>
          <div className="ds-card is-danger">
            <div className="ds-row">
              <div>
                <div className="ds-row-title">Delete this deck</div>
                <div className="ds-row-sub">Permanently removes the deck and all its cards. This can’t be undone.</div>
              </div>
              {confirmDelete ? (
                <div className="row">
                  <button
                    type="button"
                    className="btn btn-danger"
                    onClick={async () => {
                      await storage.deleteDeck(deck.id)
                      navigate('/')
                    }}
                  >
                    Confirm delete
                  </button>
                  <button type="button" className="btn btn-ghost" onClick={() => setConfirmDelete(false)}>
                    Cancel
                  </button>
                </div>
              ) : (
                <button type="button" className="btn btn-danger-outline" onClick={() => setConfirmDelete(true)}>
                  Delete deck
                </button>
              )}
            </div>
          </div>
```

- [ ] **Step 5: Add the danger-outline button style.** Append to `src/ui/styles.css`:

```css
.btn-danger-outline { border: 1px solid var(--danger); background: transparent; color: var(--danger); }
.btn-danger-outline:hover { background: color-mix(in oklch, var(--danger), transparent 88%); }
```

- [ ] **Step 6: Run page tests — expect PASS.**

Run: `npx vitest run src/features/decks/DeckSettingsPage.browser.test.tsx`
Expected: PASS.

- [ ] **Step 7: Full suite + typecheck — expect PASS.**

Run: `npm run typecheck && npm test`
Expected: PASS (unit + browser projects).

- [ ] **Step 8: Commit.**

```bash
git add src/features/decks/DeckSettingsPage.tsx src/features/decks/DeckSettingsPage.browser.test.tsx src/ui/styles.css
git commit -m "feat(decks): custom study (inert), burying/timer toggles, and danger zone

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Final verification

- [ ] Run `npm run typecheck && npm test` — all green.
- [ ] Optional GUI smoke (`npm run app:dev`): open a deck → **Options** → rename, pick a color (sidebar dot updates), bump steppers/toggles → leave and return; values persist. Delete a throwaway deck via Danger zone → returns to Today, deck gone.
- [ ] Then use **superpowers:finishing-a-development-branch** to integrate `feat/deck-options`.

## Notes for the implementer

- **Layering:** `DexieStorage`, `db.ts`, and `snapshot.ts` import `deckColor` from `src/ui/deckColor.ts`. It is a pure function (no React/DOM); this cross-layer import is intentional and keeps deck colors consistent across the app.
- **Swatches map to 5 colors,** not the comp's 6 — reusing the existing `DECK_PALETTE` so backfilled decks already match a swatch.
- **Scope boundary:** none of the persisted settings change scheduling/queue behaviour in this plan. `desiredRetention`/`maximumInterval` are stored but still sent to `fsrs_next_states` as the module default `DEFAULT_DECK_FSRS_PARAMS` until sub-project #3 threads the deck's settings through (the `DeckFsrsParams` argument already exists).
