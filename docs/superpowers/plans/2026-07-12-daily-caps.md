# Daily Caps (`newPerDay` / `maxReviews`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce each deck's `newPerDay` / `maxReviews` caps (already in `DeckSettings`) by gating which cards enter the review session, backed by persistent per-(deck, day) counters.

**Architecture:** A new local-only Dexie **v8** table `dailyStats` holds two integers per deck per local-day. A pure day-boundary helper decides "what day is now"; the pure session builder caps how many New/Review cards enter (learning/relearning are never capped); `ReviewPage` builds caps from settings minus today's counters and bumps a counter on each grade; `loadDueOverview` applies the same caps so the Today screen's counts and the All-decks queue reflect them.

**Tech Stack:** TypeScript, React, Dexie (IndexedDB), Vitest (unit + Playwright browser project). Rust/`cargo` untouched.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-12-daily-caps-design.md`.
- **Base assumption:** implement on `main` with `fix/review-followup-cleanups` merged first (so `ReviewPage.grade` already has the `gradingRef` guard + `try/finally`). Task 5 notes the plain-`main` variant if it is not merged.
- Anki-faithful: **learning (state 1) and relearning (state 3) cards are never capped.** `newPerDay` counts a card the first time it is graded out of New (pre-grade `state === 0`); `maxReviews` counts grading a card whose pre-grade `state === 2`.
- Rollover at **local midnight**; day key is `YYYY-MM-DD` in local time.
- Counters are **local-only**: do NOT touch `backup.ts`, `snapshot.ts`, or `sync/`.
- Commits authored `shettyh <manjunathshetty@live.com>` with trailer `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **Never stage/commit `features.md`.** Stage only the files named in each task.
- Tests: unit `npx vitest run --project unit <path>`; browser `npx vitest run --project browser <path>`; whole suite `npm test`; types `npm run typecheck`.

## File structure

- **Create** `src/features/review/day.ts` — `localDay(ms)` rollover helper (pure).
- **Modify** `src/domain/models.ts` — add `DailyStat` interface.
- **Modify** `src/data/dexie/db.ts` — `dailyStats` table + v8 version.
- **Modify** `src/data/Storage.ts` — `getDailyStat` / `bumpDailyStat` on the port.
- **Modify** `src/data/dexie/DexieStorage.ts` — implement both; delete a deck's stats in `deleteDeck`.
- **Modify** `src/features/review/session.ts` — `Caps` + caps arg on `buildSessionCards`.
- **Modify** `src/features/review/dueOverview.ts` — cap per-deck counts + queue.
- **Modify** `src/features/review/ReviewPage.tsx` — single-deck caps at build; bump counter on grade.

---

### Task 1: `localDay` rollover helper

**Files:**
- Create: `src/features/review/day.ts`
- Test: `src/features/review/day.test.ts`

**Interfaces:**
- Produces: `export function localDay(ms: number): string` — local calendar date as `YYYY-MM-DD`.

- [ ] **Step 1: Write the failing test**

Create `src/features/review/day.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { localDay } from './day'

describe('localDay', () => {
  it('formats a timestamp as local YYYY-MM-DD', () => {
    expect(localDay(new Date(2026, 6, 12, 9, 30).getTime())).toBe('2026-07-12')
  })

  it('rolls over at local midnight', () => {
    expect(localDay(new Date(2026, 6, 12, 23, 59).getTime())).toBe('2026-07-12')
    expect(localDay(new Date(2026, 6, 13, 0, 1).getTime())).toBe('2026-07-13')
  })
})
```

(Constructing with `new Date(year, monthIndex, ...)` uses local time on both sides, so the test is timezone-independent.)

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit src/features/review/day.test.ts`
Expected: FAIL — `localDay` is not defined / module not found.

- [ ] **Step 3: Write minimal implementation**

Create `src/features/review/day.ts`:

```ts
/** Local calendar date of a timestamp as YYYY-MM-DD (rollover at local midnight). */
export function localDay(ms: number): string {
  const d = new Date(ms)
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit src/features/review/day.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/review/day.ts src/features/review/day.test.ts
git commit -m "feat(review): local-day rollover helper for daily caps

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: `DailyStat` model + Dexie v8 table

**Files:**
- Modify: `src/domain/models.ts` (add `DailyStat` near the `Asset`/`Tombstone` interfaces)
- Modify: `src/data/dexie/db.ts` (import `DailyStat`, add table field, add v8 version)
- Test: `src/data/dexie/migration.test.ts` (add a v8 describe block)

**Interfaces:**
- Produces: `interface DailyStat { id: string; deckId: ID; day: string; newIntroduced: number; reviewsDone: number }`; `RemDB.dailyStats: EntityTable<DailyStat, 'id'>`.

- [ ] **Step 1: Write the failing test**

Add to `src/data/dexie/migration.test.ts` (after the existing describes):

```ts
describe('daily-caps migration (v8)', () => {
  it('adds an empty dailyStats table and leaves existing data intact', async () => {
    const v7 = new Dexie(NAME)
    v7.version(7).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v7.open()
    await v7.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { kind: 'fsrs', stability: 5, difficulty: 5, reps: 3, lapses: 1, state: 2, step: 0, lastReview: 100, due: 200 },
    })
    v7.close()

    const db = new RemDB(NAME)
    const card = await db.cards.get('c1')
    expect(card?.front).toBe('q')
    expect(await db.dailyStats.count()).toBe(0)
    db.close()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit src/data/dexie/migration.test.ts`
Expected: FAIL — `db.dailyStats` is undefined (property does not exist).

- [ ] **Step 3a: Add the model**

In `src/domain/models.ts`, add after the `Asset` interface:

```ts
/** Per-deck, per-day cap counters (#3b). Local-only — not synced or backed up. */
export interface DailyStat {
  id: string          // `${deckId}:${day}`
  deckId: ID
  day: string         // local calendar date, YYYY-MM-DD
  newIntroduced: number
  reviewsDone: number
}
```

- [ ] **Step 3b: Add the table + v8 version**

In `src/data/dexie/db.ts`, add `DailyStat` to the models import:

```ts
import type { Asset, Card, DailyStat, Deck, Tombstone } from '../../domain/models'
```

Add the table field alongside the others:

```ts
  dailyStats!: EntityTable<DailyStat, 'id'>
```

Add the v8 version at the end of the constructor (after the v7 block):

```ts
    // v8: add the dailyStats table for daily caps (#3b). Additive — existing data untouched.
    this.version(8).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
      dailyStats: 'id, deckId, day',
    })
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit src/data/dexie/migration.test.ts`
Expected: PASS (existing migration tests + the new v8 test).

- [ ] **Step 5: Commit**

```bash
git add src/domain/models.ts src/data/dexie/db.ts src/data/dexie/migration.test.ts
git commit -m "feat(data): DailyStat model + Dexie v8 dailyStats table

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Storage seam — `getDailyStat` / `bumpDailyStat`

**Files:**
- Modify: `src/data/Storage.ts` (add two methods to the `Storage` interface)
- Modify: `src/data/dexie/DexieStorage.ts` (implement both; add `dailyStats` to `deleteDeck`)
- Test: `src/data/dexie/DexieStorage.test.ts` (new `daily stats` describe block)

**Interfaces:**
- Consumes: `RemDB.dailyStats` (Task 2).
- Produces:
  - `getDailyStat(deckId: ID, day: string): Promise<{ newIntroduced: number; reviewsDone: number }>` — zeros when no row.
  - `bumpDailyStat(deckId: ID, day: string, field: 'newIntroduced' | 'reviewsDone'): Promise<void>` — upsert +1.

- [ ] **Step 1: Write the failing test**

Add to `src/data/dexie/DexieStorage.test.ts`:

```ts
describe('daily stats', () => {
  it('returns zeros when no row exists', async () => {
    expect(await storage.getDailyStat('d1', '2026-07-12')).toEqual({ newIntroduced: 0, reviewsDone: 0 })
  })

  it('bumps and accumulates counters per (deck, day)', async () => {
    await storage.bumpDailyStat('d1', '2026-07-12', 'newIntroduced')
    await storage.bumpDailyStat('d1', '2026-07-12', 'newIntroduced')
    await storage.bumpDailyStat('d1', '2026-07-12', 'reviewsDone')
    expect(await storage.getDailyStat('d1', '2026-07-12')).toEqual({ newIntroduced: 2, reviewsDone: 1 })
  })

  it('keeps different days separate', async () => {
    await storage.bumpDailyStat('d1', '2026-07-12', 'reviewsDone')
    expect(await storage.getDailyStat('d1', '2026-07-13')).toEqual({ newIntroduced: 0, reviewsDone: 0 })
  })

  it('deleteDeck removes the deck stats', async () => {
    const deck = await storage.createDeck('S')
    await storage.bumpDailyStat(deck.id, '2026-07-12', 'newIntroduced')
    await storage.deleteDeck(deck.id)
    expect(await storage.getDailyStat(deck.id, '2026-07-12')).toEqual({ newIntroduced: 0, reviewsDone: 0 })
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit src/data/dexie/DexieStorage.test.ts`
Expected: FAIL — `storage.getDailyStat` is not a function.

- [ ] **Step 3a: Extend the port**

In `src/data/Storage.ts`, add to the `Storage` interface (after `countDue`):

```ts
  /** Today's cap counters for a deck; zeros when the day has no row yet. */
  getDailyStat(deckId: ID, day: string): Promise<{ newIntroduced: number; reviewsDone: number }>
  /** Increment one of a deck's daily counters by 1 (upsert). */
  bumpDailyStat(deckId: ID, day: string, field: 'newIntroduced' | 'reviewsDone'): Promise<void>
```

- [ ] **Step 3b: Implement in DexieStorage + extend deleteDeck**

In `src/data/dexie/DexieStorage.ts`, add the two methods (e.g. after `countDue`):

```ts
  async getDailyStat(deckId: ID, day: string): Promise<{ newIntroduced: number; reviewsDone: number }> {
    const row = await this.db.dailyStats.get(`${deckId}:${day}`)
    return { newIntroduced: row?.newIntroduced ?? 0, reviewsDone: row?.reviewsDone ?? 0 }
  }

  async bumpDailyStat(deckId: ID, day: string, field: 'newIntroduced' | 'reviewsDone'): Promise<void> {
    const id = `${deckId}:${day}`
    await this.db.transaction('rw', this.db.dailyStats, async () => {
      const row = await this.db.dailyStats.get(id)
      const base = row ?? { id, deckId, day, newIntroduced: 0, reviewsDone: 0 }
      await this.db.dailyStats.put({ ...base, [field]: base[field] + 1 })
    })
  }
```

Extend `deleteDeck` to also clear the deck's stats — add `this.db.dailyStats` to the transaction tables and a delete:

```ts
  async deleteDeck(id: ID): Promise<void> {
    await this.db.transaction('rw', this.db.decks, this.db.cards, this.db.tombstones, this.db.dailyStats, async () => {
      await this.db.cards.where('deckId').equals(id).delete()
      await this.db.dailyStats.where('deckId').equals(id).delete()
      await this.db.decks.delete(id)
      await this.db.tombstones.put({ id, kind: 'deck', deletedAt: Date.now() })
    })
  }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit src/data/dexie/DexieStorage.test.ts`
Then: `npm run typecheck`
Expected: PASS; typecheck clean (the `Storage` interface and `DexieStorage` agree).

- [ ] **Step 5: Commit**

```bash
git add src/data/Storage.ts src/data/dexie/DexieStorage.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat(data): daily-stat counters on the storage port

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: `buildSessionCards` caps (pure)

**Files:**
- Modify: `src/features/review/session.ts` (export `Caps`; add caps arg + 3-way partition)
- Test: `src/features/review/session.test.ts` (new `buildSessionCards caps` describe block)

**Interfaces:**
- Produces:
  - `export interface Caps { newSlots: number; reviewSlots: number }`
  - `buildSessionCards(cards: SessionCard[], order: InsertionOrder, caps?: Caps): SessionCard[]`
- Note: `caps` is **optional, defaulting to uncapped** (`{ newSlots: Infinity, reviewSlots: Infinity }`) so existing two-arg callers stay valid until Tasks 5–6 pass real caps. Uncapped behavior is byte-identical to today's builder.

- [ ] **Step 1: Write the failing test**

Add to `src/features/review/session.test.ts` (the file's `card`/`sched` helpers already exist):

```ts
describe('buildSessionCards caps', () => {
  it('caps new cards to newSlots by insertion order', () => {
    const cards = [card('n1', 10, sched(0, 0)), card('n2', 20, sched(0, 0)), card('n3', 30, sched(0, 0))]
    const out = buildSessionCards(cards, 'sequential', { newSlots: 2, reviewSlots: Infinity }).map((c) => c.card.id)
    expect(out).toEqual(['n1', 'n2'])
  })

  it('caps review cards to reviewSlots, keeping earliest due', () => {
    const cards = [card('r1', 1, sched(2, 100)), card('r2', 2, sched(2, 200)), card('r3', 3, sched(2, 300))]
    const out = buildSessionCards(cards, 'sequential', { newSlots: Infinity, reviewSlots: 2 }).map((c) => c.card.id)
    expect(out).toEqual(['r1', 'r2'])
  })

  it('never caps in-progress learning/relearning cards', () => {
    const cards = [card('l1', 1, sched(1, 0)), card('rl1', 2, sched(3, 0)), card('n1', 3, sched(0, 0))]
    const out = buildSessionCards(cards, 'sequential', { newSlots: 0, reviewSlots: 0 }).map((c) => c.card.id).sort()
    expect(out).toEqual(['l1', 'rl1'])
  })

  it('treats negative slots as zero', () => {
    const cards = [card('n1', 1, sched(0, 0)), card('r1', 2, sched(2, 0))]
    expect(buildSessionCards(cards, 'sequential', { newSlots: -3, reviewSlots: -1 })).toEqual([])
  })

  it('defaults to uncapped when caps omitted', () => {
    const cards = [card('n1', 1, sched(0, 0)), card('n2', 2, sched(0, 0)), card('r1', 3, sched(2, 0))]
    const out = buildSessionCards(cards, 'sequential').map((c) => c.card.id)
    expect(out).toEqual(['r1', 'n1', 'n2'])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit src/features/review/session.test.ts`
Expected: FAIL — the caps cases fail (e.g. new not capped) because the third arg is ignored today.

- [ ] **Step 3: Implement caps in the builder**

In `src/features/review/session.ts`, add the `Caps` interface (near `SessionCard`) and replace `buildSessionCards`:

```ts
export interface Caps {
  newSlots: number
  reviewSlots: number
}

/** Initial single-deck order: in-progress + due reviews first (by due), then new
 *  cards in insertion order. `caps` bounds how many New (state 0) and Review
 *  (state 2) cards enter; learning/relearning (state 1/3) are never capped. */
export function buildSessionCards(
  cards: SessionCard[],
  order: InsertionOrder,
  caps: Caps = { newSlots: Infinity, reviewSlots: Infinity },
): SessionCard[] {
  const newSlots = Math.max(0, caps.newSlots)
  const reviewSlots = Math.max(0, caps.reviewSlots)

  const news = cards.filter((c) => c.card.scheduling.state === 0)
  const reviews = cards.filter((c) => c.card.scheduling.state === 2)
  const inProgress = cards.filter(
    (c) => c.card.scheduling.state === 1 || c.card.scheduling.state === 3,
  )

  const orderedNew = (
    order === 'random'
      ? shuffle(news)
      : news.slice().sort((a, b) => a.card.createdAt - b.card.createdAt)
  ).slice(0, newSlots)

  const keptReview = reviews
    .slice()
    .sort((a, b) => a.card.scheduling.due - b.card.scheduling.due)
    .slice(0, reviewSlots)

  const orderedRest = [...inProgress, ...keptReview].sort(
    (a, b) => a.card.scheduling.due - b.card.scheduling.due,
  )

  return [...orderedRest, ...orderedNew]
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit src/features/review/session.test.ts`
Then: `npm run typecheck`
Expected: PASS (new caps cases + all pre-existing session tests, which use the uncapped default); typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add src/features/review/session.ts src/features/review/session.test.ts
git commit -m "feat(review): cap new/review cards in the session builder

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: `ReviewPage` — single-deck caps + grade counter bump

**Files:**
- Modify: `src/features/review/ReviewPage.tsx`
- Test: `src/features/review/reveal.browser.test.tsx`

**Interfaces:**
- Consumes: `localDay` (Task 1), `getDailyStat` / `bumpDailyStat` (Task 3), `buildSessionCards(cards, order, caps)` (Task 4).

> **Base note:** snippets below assume `fix/review-followup-cleanups` is merged (so `grade` has the `gradingRef` guard + `try/finally`). If implementing on plain `main`, `grade` has no guard — put the two `bumpDailyStat` lines immediately after `await storage.updateCard(...)` in the flat body instead.

- [ ] **Step 1: Write the failing test**

In `src/features/review/reveal.browser.test.tsx`, add these imports at the top:

```ts
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { localDay } from './day'
```

Add the test:

```ts
test('enforces newPerDay: only the day\'s new allowance enters the session', async () => {
  const storage = freshStorage()
  const deck = await storage.createDeck('Caps')
  await storage.updateDeck(deck.id, {
    settings: { ...DEFAULT_DECK_SETTINGS, newPerDay: 1, learnSteps: '1m' },
  })
  await storage.createCard(deck.id, 'Q1', 'A1')
  await storage.createCard(deck.id, 'Q2', 'A2')
  await renderRoute({
    storage,
    entry: `/decks/${deck.id}/study`,
    path: '/decks/:deckId/study',
    element: <ReviewPage />,
  })

  // Two new cards are due, but only one enters: position reads "1 / 1", not "1 / 2".
  await expect.element(page.getByText('1 / 1', { exact: false })).toBeVisible()

  // Grading it out of New bumps newIntroduced.
  await page.getByRole('button', { name: 'Show answer', exact: false }).click()
  await page.getByRole('button', { name: 'Good', exact: false }).click()
  await vi.waitFor(async () => {
    const stat = await storage.getDailyStat(deck.id, localDay(Date.now()))
    expect(stat.newIntroduced).toBe(1)
  })
})
```

(`learnSteps: '1m'` is a single learning step, so one `Good` graduates the card.)

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project browser src/features/review/reveal.browser.test.tsx`
Expected: FAIL — position reads `1 / 2` (both new cards enter; no cap yet).

- [ ] **Step 3a: Cap the single-deck build**

In `src/features/review/ReviewPage.tsx`, add the import:

```ts
import { localDay } from './day'
```

Replace the single-deck branch of `build()`:

```ts
      if (deckId) {
        const d = await storage.getDeck(deckId)
        if (!d) return []
        const due = await storage.dueCards(deckId, now)
        const stat = await storage.getDailyStat(deckId, localDay(now))
        const caps = {
          newSlots: d.settings.newPerDay - stat.newIntroduced,
          reviewSlots: d.settings.maxReviews - stat.reviewsDone,
        }
        const cards = due.map((card) => ({ card, settings: d.settings }))
        return buildSessionCards(cards, d.settings.insertionOrder, caps)
      }
```

- [ ] **Step 3b: Bump the counter on grade**

In the `grade` callback, inside the `try` (guard base), capture the pre-grade state and bump after the card persists:

```ts
      const preState = current.card.scheduling.state
      await storage.updateCard(current.card.id, { scheduling: nexts[g] })
      const day = localDay(Date.now())
      if (preState === 0) await storage.bumpDailyStat(current.card.deckId, day, 'newIntroduced')
      else if (preState === 2) await storage.bumpDailyStat(current.card.deckId, day, 'reviewsDone')
      session.grade(Date.now(), nexts[g])
```

(Bump by `current.card.deckId`, not the route `deckId`, so All-decks review counts against each card's own deck.)

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project browser src/features/review/reveal.browser.test.tsx`
Then: `npm run typecheck`
Expected: PASS (new cap test + the existing reveal/guard tests); typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add src/features/review/ReviewPage.tsx src/features/review/reveal.browser.test.tsx
git commit -m "feat(review): enforce daily caps in single-deck review + count grades

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: `loadDueOverview` — capped counts + All-decks queue

**Files:**
- Modify: `src/features/review/dueOverview.ts`
- Test: `src/features/review/dueOverview.test.ts` (extend `fakeStorage`; add capping cases)

**Interfaces:**
- Consumes: `localDay` (Task 1), `storage.getDailyStat` (Task 3), `buildSessionCards` + `Caps` (Task 4).
- Produces: `loadDueOverview` returning capped per-deck `newCount`/`due`, capped aggregate `totalNew`/`totalReview`/`totalDue`, and a capped cross-deck `queue`. The `totalDue = totalNew + totalReview` invariant is preserved. `ReviewPage`'s All-decks path consumes the now-capped `queue` unchanged.

- [ ] **Step 1: Write the failing test**

In `src/features/review/dueOverview.test.ts`, extend `fakeStorage` to stub `getDailyStat` (add a `stats` param):

```ts
function fakeStorage(
  decks: Deck[],
  cards: Card[],
  dueIds: Set<ID>,
  stats: Map<ID, { newIntroduced: number; reviewsDone: number }> = new Map(),
): Storage {
  return {
    listDecks: async () => decks,
    listCards: async (deckId: ID) => cards.filter((c) => c.deckId === deckId),
    dueCards: async (deckId: ID) => cards.filter((c) => c.deckId === deckId && dueIds.has(c.id)),
    getDailyStat: async (deckId: ID) => stats.get(deckId) ?? { newIntroduced: 0, reviewsDone: 0 },
  } as unknown as Storage
}
```

Add capping cases inside `describe('loadDueOverview', ...)`:

```ts
  it('caps new and review counts by the deck limits', async () => {
    const d = deck('a')
    d.settings = { ...DEFAULT_DECK_SETTINGS, newPerDay: 1, maxReviews: 1 }
    const cards = [
      card('n1', 'a', fsrs(0)), card('n2', 'a', fsrs(0)),
      card('r1', 'a', fsrs(4)), card('r2', 'a', fsrs(4)),
    ]
    const due = new Set(['n1', 'n2', 'r1', 'r2'])
    const ov = await loadDueOverview(fakeStorage([d], cards, due), Date.now())
    expect(ov.totalNew).toBe(1)
    expect(ov.totalReview).toBe(1)
    expect(ov.totalDue).toBe(2)
    expect(ov.decks[0]).toMatchObject({ due: 2, newCount: 1 })
  })

  it('subtracts allowance already spent today', async () => {
    const d = deck('a')
    d.settings = { ...DEFAULT_DECK_SETTINGS, newPerDay: 2, maxReviews: 5 }
    const cards = [card('n1', 'a', fsrs(0)), card('n2', 'a', fsrs(0))]
    const due = new Set(['n1', 'n2'])
    const stats = new Map([['a', { newIntroduced: 2, reviewsDone: 0 }]])
    const ov = await loadDueOverview(fakeStorage([d], cards, due, stats), Date.now())
    expect(ov.totalNew).toBe(0)
  })
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit src/features/review/dueOverview.test.ts`
Expected: FAIL — `totalNew`/`totalReview` still show the uncapped 2/2 (and `getDailyStat` is now referenced but the impl doesn't call it yet).

- [ ] **Step 3: Apply caps in `loadDueOverview`**

In `src/features/review/dueOverview.ts`, add imports:

```ts
import { localDay } from './day'
import { buildSessionCards } from './session'
```

Replace the body of `loadDueOverview`:

```ts
export async function loadDueOverview(storage: Storage, now: number): Promise<DueOverview> {
  const allDecks = await storage.listDecks()
  const day = localDay(now)
  const decks: DeckOverview[] = []
  const queue: Card[] = []

  for (const deck of allDecks) {
    const [cards, dueList, stat] = await Promise.all([
      storage.listCards(deck.id),
      storage.dueCards(deck.id, now),
      storage.getDailyStat(deck.id, day),
    ])
    const caps = {
      newSlots: deck.settings.newPerDay - stat.newIntroduced,
      reviewSlots: deck.settings.maxReviews - stat.reviewsDone,
    }
    const capped = buildSessionCards(
      dueList.map((card) => ({ card, settings: deck.settings })),
      deck.settings.insertionOrder,
      caps,
    ).map((c) => c.card)

    decks.push({
      deck,
      due: capped.length,
      newCount: capped.filter((c) => isNew(c.scheduling)).length,
      total: cards.length,
    })
    queue.push(...capped)
  }

  const totalDue = queue.length
  const totalNew = queue.filter((c) => isNew(c.scheduling)).length
  return { decks, queue, totalDue, totalNew, totalReview: totalDue - totalNew }
}
```

(Keep the existing `isNew` and `shuffle` exports unchanged.)

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit src/features/review/dueOverview.test.ts`
Then: `npm run typecheck`
Expected: PASS (new capping cases + the pre-existing aggregate/empty cases, which stay uncapped under the default-20/200 settings); typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add src/features/review/dueOverview.ts src/features/review/dueOverview.test.ts
git commit -m "feat(review): cap Today-screen counts and the All-decks queue

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Wrap-up (after Task 6)

- [ ] Run the whole suite: `npm test` — expect all green (unit + browser).
- [ ] `npm run typecheck` — clean.
- [ ] `grep -rn "backup\|snapshot" src/features/review src/data/dexie/DexieStorage.ts` sanity: confirm no counter logic leaked into backup/snapshot/sync (counters are local-only).
- [ ] Manual smoke (`npm run app:dev`): set a deck to `newPerDay 2`, study — exactly two new cards appear; the Today screen shows capped counts. (Native/manual only; automated tests use the fake scheduler + Dexie.)
- [ ] Hand off to **superpowers:finishing-a-development-branch** to integrate `feat/daily-caps`.

## Self-review (done while writing)

- **Spec coverage:** [D1] counters → Tasks 2–3; [D2] local-midnight day → Task 1; [D3] what-counts → Task 5 grade (pre-state 0/2) + Task 4 partition; [D4] build-time gating + durable counters → Tasks 4–5; [D5] single-deck (Task 5) + All-decks (Task 6 via capped queue); [D6] Today counts → Task 6. Migration, deleteDeck cleanup, local-only (no backup/snapshot) all covered.
- **Placeholder scan:** none — every step has concrete code/commands.
- **Type consistency:** `Caps { newSlots, reviewSlots }`, `getDailyStat → { newIntroduced, reviewsDone }`, `bumpDailyStat(deckId, day, field)`, `localDay(ms): string`, `DailyStat` fields — used identically across Tasks 2–6.
</content>
