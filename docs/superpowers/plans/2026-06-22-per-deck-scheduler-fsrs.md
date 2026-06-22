# Per-deck scheduler (SM-2 + FSRS) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add FSRS (via the `ts-fsrs` library) as a second spaced-repetition algorithm, chosen per deck at creation time and fixed thereafter; existing decks keep SM-2.

**Architecture:** `SchedulingState` becomes a discriminated union (`SM2State | FSRSState`) keyed on `kind`, so each card is self-describing. `Deck` gains a `schedulerKind` read only at card creation. A `getScheduler(kind)` registry replaces the singleton; review/grade dispatch resolves the scheduler from `card.scheduling.kind`. FSRS is a thin adapter over `ts-fsrs`.

**Tech Stack:** TypeScript, React 19, Dexie (IndexedDB), Vitest (unit + Playwright browser projects), `ts-fsrs`.

## Global Constraints

- All timestamps are epoch milliseconds (`number`).
- Schedulers are pure: `initial(now)` / `next(state, grade, now)` return new state; no I/O, no `Date.now()` inside.
- Grades are exactly `'again' | 'hard' | 'good' | 'easy'`.
- Day-granular scheduling everywhere (no sub-day intervals).
- FSRS uses default global weights with `enable_fuzz: false` (deterministic) and `enable_short_term: false` (day-granular); `request_retention: 0.9`. No optimizer.
- New decks default to FSRS in the UI; `Storage.createDeck`'s `kind` parameter defaults to `'sm2'` (keeps existing call sites unchanged).
- Backup file format stays `version: 1` (backward-compatible; legacy files normalized on read).
- Match existing code style. Run `npm run typecheck` and `npm test` after each task; both must be green before committing.
- Do not commit or revert the pre-existing uncommitted working-tree changes (`DeckListPage.tsx` home-hero, `DeckDetailPage.tsx`, `styles.css`, `cardPreview.test.ts`); leave them in place and build alongside them.

---

## File map

- `src/domain/models.ts` — modify: `SchedulerKind`, `SM2State`, `FSRSState`, `SchedulingState` union, `Deck.schedulerKind`.
- `src/domain/scheduler/Scheduler.ts` — unchanged (interface already fits).
- `src/domain/scheduler/sm2.ts` — modify: stamp `kind: 'sm2'`, narrow input.
- `src/domain/scheduler/fsrs.ts` — create: `FSRSScheduler` adapter.
- `src/domain/scheduler/index.ts` — modify: `getScheduler(kind)` registry; remove `scheduler` singleton.
- `src/data/Storage.ts` — modify: `createDeck(name, kind?)`.
- `src/data/dexie/db.ts` — modify: Dexie `version(2)` upgrade.
- `src/data/dexie/DexieStorage.ts` — modify: per-deck scheduler at `createCard`; set `schedulerKind` on create/import; drop injected-scheduler arg.
- `src/data/StorageContext.tsx` — modify: drop scheduler arg.
- `src/data/backup.ts` — modify: `DeckBackup.schedulerKind`, union validation, legacy normalize.
- `src/test/seed.ts` — modify: drop scheduler arg.
- `src/features/cards/DeckDetailPage.tsx` — modify: `cardStatus` kind switch; algorithm badge.
- `src/features/decks/DeckListPage.tsx` — modify: algorithm picker in create form.
- `src/features/review/GradeButtons.tsx` — modify: interval from `due`; resolve scheduler by kind.
- `src/features/review/ReviewPage.tsx` — modify: resolve scheduler by kind.
- `src/ui/styles.css` — modify: picker + badge styles.
- Tests touched/added: `sm2.test.ts`, `fsrs.test.ts` (new), `cardStatus.test.ts`, `backup.test.ts`, `DexieStorage.test.ts`, `reviewCycle.test.ts`, `db` migration test (new), `DeckListPage` scheduler-pick test (new).

---

## Task 1: Self-describing scheduling state (card-level discriminant)

Introduce the `SchedulingState` discriminated union and make SM-2 stamp it. No `Deck` changes yet, no FSRS algorithm yet — `FSRSState` is defined so consumers can handle it, but nothing produces it. Behavior is unchanged.

**Files:**
- Modify: `src/domain/models.ts`
- Modify: `src/domain/scheduler/sm2.ts`
- Modify: `src/domain/scheduler/sm2.test.ts`
- Modify: `src/features/cards/DeckDetailPage.tsx` (`cardStatus` only)
- Modify: `src/features/cards/cardStatus.test.ts`
- Modify: `src/features/review/GradeButtons.tsx`
- Modify: `src/data/backup.ts` (`isScheduling`, `parseCard` normalize)
- Modify: `src/data/backup.test.ts`
- Modify: `src/data/dexie/DexieStorage.test.ts` (narrow assertions; `kind` on literals)

**Interfaces:**
- Produces: `type SchedulerKind = 'sm2' | 'fsrs'`; `interface SM2State { kind: 'sm2'; repetitions: number; intervalDays: number; easeFactor: number; due: number }`; `interface FSRSState { kind: 'fsrs'; stability: number; difficulty: number; reps: number; lapses: number; state: number; lastReview: number | null; due: number }`; `type SchedulingState = SM2State | FSRSState`.

- [ ] **Step 1: Update the data model**

In `src/domain/models.ts`, replace the `SchedulingState` interface (lines 14–24) with:

```ts
/** Which scheduling algorithm owns a deck's cards. */
export type SchedulerKind = 'sm2' | 'fsrs'

/** SM-2 per-card scheduling state. */
export interface SM2State {
  kind: 'sm2'
  /** Number of consecutive successful reviews. */
  repetitions: number
  /** Current inter-review interval in days. */
  intervalDays: number
  /** SM-2 ease factor (>= 1.3). */
  easeFactor: number
  /** When the card is next due (epoch ms). */
  due: number
}

/** FSRS per-card scheduling state. */
export interface FSRSState {
  kind: 'fsrs'
  /** Memory stability in days. */
  stability: number
  /** Card difficulty (1–10). */
  difficulty: number
  /** Total reviews so far. */
  reps: number
  /** Number of failed reviews. */
  lapses: number
  /** ts-fsrs State enum: 0 New / 1 Learning / 2 Review / 3 Relearning. */
  state: number
  /** Last review time (epoch ms), or null if never reviewed. */
  lastReview: number | null
  /** When the card is next due (epoch ms). */
  due: number
}

/** Per-card scheduling state, owned by the deck's scheduling algorithm. */
export type SchedulingState = SM2State | FSRSState
```

- [ ] **Step 2: Stamp `kind` in SM-2**

In `src/domain/scheduler/sm2.ts`, update `initial` and `next` to include `kind: 'sm2'`, and narrow the input in `next`:

```ts
  initial(now: number): SchedulingState {
    return { kind: 'sm2', repetitions: 0, intervalDays: 0, easeFactor: INITIAL_EASE, due: now }
  }

  next(state: SchedulingState, grade: Grade, now: number): SchedulingState {
    if (state.kind !== 'sm2') throw new Error('SM2Scheduler received non-SM-2 state')
    const q = QUALITY[grade]
    const easeFactor = Math.max(
      MIN_EASE,
      state.easeFactor + (0.1 - (5 - q) * (0.08 + (5 - q) * 0.02)),
    )

    let repetitions: number
    let intervalDays: number
    if (q < 3) {
      repetitions = 0
      intervalDays = 1
    } else if (state.repetitions === 0) {
      repetitions = 1
      intervalDays = 1
    } else if (state.repetitions === 1) {
      repetitions = 2
      intervalDays = 6
    } else {
      repetitions = state.repetitions + 1
      intervalDays = Math.round(state.intervalDays * easeFactor)
    }

    return { kind: 'sm2', repetitions, intervalDays, easeFactor, due: now + intervalDays * MS_PER_DAY }
  }
```

- [ ] **Step 3: Update SM-2 tests for `kind`**

In `src/domain/scheduler/sm2.test.ts`, update the `initial` assertion (line 9) to expect `kind: 'sm2'`:

```ts
    expect(scheduler.initial(now)).toEqual({
      kind: 'sm2',
      repetitions: 0,
      intervalDays: 0,
      easeFactor: 2.5,
      due: now,
    })
```

The other tests assert individual fields (`.repetitions`, `.intervalDays`, etc.); to read those off the union, change each `const s = scheduler.next(...)` usage that touches SM-2-only fields to narrow first. Simplest: at the top of each `it` that reads `.repetitions`/`.intervalDays`/`.easeFactor`, wrap access with a kind check. Replace each such assertion block; e.g. the first progression test becomes:

```ts
  it('schedules a new card 1 day out on the first "good"', () => {
    const s = scheduler.next(scheduler.initial(now), 'good', now)
    if (s.kind !== 'sm2') throw new Error('expected sm2')
    expect(s.repetitions).toBe(1)
    expect(s.intervalDays).toBe(1)
    expect(s.due).toBe(now + 1 * MS_PER_DAY)
  })
```

Apply the same `if (s.kind !== 'sm2') throw new Error('expected sm2')` guard (using the variable name in scope — `s`, `first`, `second`, `lapsed`) before any `.repetitions`/`.intervalDays`/`.easeFactor` access in the remaining tests.

- [ ] **Step 4: Run SM-2 tests**

Run: `npx vitest run src/domain/scheduler/sm2.test.ts`
Expected: PASS.

- [ ] **Step 5: Make `cardStatus` kind-aware**

In `src/features/cards/DeckDetailPage.tsx`, replace the "new" check in `cardStatus` (line 25) so it works on the union:

```ts
  const isNew = s.kind === 'sm2' ? s.repetitions === 0 : s.reps === 0
  if (isNew) return { kind: 'new', label: 'new' }
```

(Leave the `due`/`scheduled` logic below it unchanged.)

- [ ] **Step 6: Add an FSRS case to the cardStatus test**

In `src/features/cards/cardStatus.test.ts`, add `kind: 'sm2'` to each existing scheduling literal (lines 9, 12, 15) and add a new test:

```ts
  it('marks an unreviewed FSRS card new', () => {
    const s = { kind: 'fsrs' as const, stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, lastReview: null, due: now }
    expect(cardStatus(s, now).kind).toBe('new')
  })

  it('marks a reviewed FSRS card due when past its due date', () => {
    const s = { kind: 'fsrs' as const, stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, lastReview: now - MS_PER_DAY, due: now - 1 }
    expect(cardStatus(s, now).kind).toBe('due')
  })
```

- [ ] **Step 7: Compute the grade-button interval from `due`**

In `src/features/review/GradeButtons.tsx`, replace the `grade-hint` line (line 37), since `.intervalDays` no longer exists on the union:

```tsx
          <span className="grade-hint">
            {formatInterval(
              Math.max(1, Math.round((scheduler.next(scheduling, grade, now).due - now) / MS_PER_DAY)),
            )}
          </span>
```

Add the import at the top of the file:

```ts
import { scheduler, MS_PER_DAY } from '../../domain/scheduler'
```

(Replace the existing `import { scheduler } from '../../domain/scheduler'`.)

- [ ] **Step 8: Normalize legacy scheduling on backup parse**

In `src/data/backup.ts`, replace `isScheduling` (lines 127–135) and update `parseCard` to normalize. New `isScheduling`:

```ts
function isScheduling(v: unknown): v is SchedulingState {
  if (!isObject(v) || typeof v.due !== 'number') return false
  if (v.kind === 'fsrs') {
    return (
      typeof v.stability === 'number' &&
      typeof v.difficulty === 'number' &&
      typeof v.reps === 'number' &&
      typeof v.lapses === 'number' &&
      typeof v.state === 'number' &&
      (v.lastReview === null || typeof v.lastReview === 'number')
    )
  }
  // sm2 (kind 'sm2' or legacy/absent)
  return (
    typeof v.repetitions === 'number' &&
    typeof v.intervalDays === 'number' &&
    typeof v.easeFactor === 'number'
  )
}

/** Stamp a `kind` onto legacy (pre-discriminant) scheduling state. */
function normalizeScheduling(v: SchedulingState): SchedulingState {
  if (v.kind === 'fsrs') return v
  return { ...v, kind: 'sm2' }
}
```

In `parseCard`, change the returned `scheduling` (line 123) to `scheduling: normalizeScheduling(raw.scheduling)`. (`raw.scheduling` is already narrowed to `SchedulingState` by the `isScheduling` guard above.)

- [ ] **Step 9: Update backup tests for `kind`**

In `src/data/backup.test.ts`:
- Add `kind: 'sm2'` to the `sched` literal (line 12): `const sched = { kind: 'sm2' as const, repetitions: 1, intervalDays: 3, easeFactor: 2.6, due: 999 }`.
- Add a legacy-normalization test inside `describe('parseBackup', ...)`:

```ts
  it('stamps kind sm2 onto legacy scheduling without a kind', () => {
    const legacy = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, cards: [
        { front: 'a', back: 'b', createdAt: 1, updatedAt: 1, scheduling: { repetitions: 1, intervalDays: 3, easeFactor: 2.6, due: 999 } },
      ] }],
    })
    expect(parseBackup(legacy)[0].cards[0].scheduling).toEqual({ kind: 'sm2', repetitions: 1, intervalDays: 3, easeFactor: 2.6, due: 999 })
  })

  it('accepts FSRS scheduling', () => {
    const f = { kind: 'fsrs', stability: 5, difficulty: 5, reps: 1, lapses: 0, state: 2, lastReview: 1, due: 2 }
    const file = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, cards: [{ front: 'a', back: 'b', createdAt: 1, updatedAt: 1, scheduling: f }] }],
    })
    expect(parseBackup(file)[0].cards[0].scheduling).toEqual(f)
  })
```

- [ ] **Step 10: Fix DexieStorage test type errors from the union**

In `src/data/dexie/DexieStorage.test.ts`:
- Replace the createCard assertions (lines 50–51) with a `toMatchObject` to avoid narrowing noise:

```ts
    expect(card.scheduling).toMatchObject({ kind: 'sm2', easeFactor: 2.5, repetitions: 0 })
    expect(card.scheduling.due).toBeGreaterThanOrEqual(before)
```

- Add `kind: 'sm2'` to the updateCard scheduling literal (line 85): `scheduling: { kind: 'sm2', repetitions: 1, intervalDays: 1, easeFactor: 2.5, due: now + MS_PER_DAY }`.
- Add `kind: 'sm2'` to the importDecks scheduling literals (lines 112, 134) and to the expected value (line 123): `expect(cards[0].scheduling).toEqual({ kind: 'sm2', repetitions: 2, intervalDays: 4, easeFactor: 2.7, due: 8 })`.

(The `new DexieStorage(db, new SM2Scheduler())` line stays as-is in this task.)

- [ ] **Step 11: Typecheck and run the full suite**

Run: `npm run typecheck && npm test`
Expected: PASS (all existing behavior preserved; new tests green).

- [ ] **Step 12: Commit**

```bash
git add src/domain/models.ts src/domain/scheduler/sm2.ts src/domain/scheduler/sm2.test.ts \
  src/features/cards/DeckDetailPage.tsx src/features/cards/cardStatus.test.ts \
  src/features/review/GradeButtons.tsx src/data/backup.ts src/data/backup.test.ts \
  src/data/dexie/DexieStorage.test.ts
git commit -m "feat: discriminated SchedulingState union (sm2 self-describing)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: `Deck.schedulerKind` + persistence and backup of the deck's algorithm

Add the deck-level algorithm tag, stored on create/import and round-tripped through backup. `createCard` still uses the injected SM-2 scheduler (FSRS isn't wired yet); this task only makes decks *carry* their algorithm.

**Files:**
- Modify: `src/domain/models.ts` (`Deck`)
- Modify: `src/data/Storage.ts` (`createDeck` signature)
- Modify: `src/data/dexie/DexieStorage.ts` (`createDeck`, `importDecks`)
- Modify: `src/data/backup.ts` (`DeckBackup`, `collectBackup`, `parseDeck`)
- Modify: `src/data/backup.test.ts`
- Modify: `src/data/dexie/DexieStorage.test.ts`

**Interfaces:**
- Consumes: `SchedulerKind` from Task 1.
- Produces: `Deck.schedulerKind: SchedulerKind`; `Storage.createDeck(name: string, kind?: SchedulerKind): Promise<Deck>` (defaults `'sm2'`); `DeckBackup.schedulerKind: SchedulerKind`.

- [ ] **Step 1: Add `schedulerKind` to `Deck`**

In `src/domain/models.ts`, add to the `Deck` interface:

```ts
export interface Deck {
  id: ID
  name: string
  createdAt: number
  /** Which scheduling algorithm this deck's cards use (fixed at creation). */
  schedulerKind: SchedulerKind
}
```

- [ ] **Step 2: Widen the `createDeck` port signature**

In `src/data/Storage.ts`, change line 25 and import `SchedulerKind`:

```ts
import type { Card, Deck, ID, SchedulerKind, SchedulingState } from '../domain/models'
```
```ts
  createDeck(name: string, kind?: SchedulerKind): Promise<Deck>
```

- [ ] **Step 3: Set `schedulerKind` in DexieStorage create/import**

In `src/data/dexie/DexieStorage.ts`:
- Import `SchedulerKind`: change line 1 to `import type { Card, Deck, ID, SchedulerKind } from '../../domain/models'`.
- Update `createDeck` (lines 14–22):

```ts
  async createDeck(name: string, kind: SchedulerKind = 'sm2'): Promise<Deck> {
    const deck: Deck = {
      id: crypto.randomUUID(),
      name: name.trim(),
      createdAt: Date.now(),
      schedulerKind: kind,
    }
    await this.db.decks.add(deck)
    return deck
  }
```

- In `importDecks`, set the kind when adding a deck (line 97):

```ts
        await this.db.decks.add({ id: deckId, name: d.name, createdAt: d.createdAt, schedulerKind: d.schedulerKind })
```

- [ ] **Step 4: Carry `schedulerKind` through backup**

In `src/data/backup.ts`:
- Import `SchedulerKind`: change line 1 to `import type { ID, SchedulerKind, SchedulingState } from '../domain/models'`.
- Add the field to `DeckBackup`:

```ts
export interface DeckBackup {
  name: string
  createdAt: number
  schedulerKind: SchedulerKind
  cards: CardBackup[]
}
```

- In `collectBackup`, include it in the pushed object (after `createdAt`, line 35): `schedulerKind: deck.schedulerKind,`.
- In `parseDeck`, default legacy decks to `'sm2'`; change the return (line 104):

```ts
  return {
    name: raw.name,
    createdAt: raw.createdAt,
    schedulerKind: raw.schedulerKind === 'fsrs' ? 'fsrs' : 'sm2',
    cards: raw.cards.map(parseCard),
  }
```

- [ ] **Step 5: Update backup tests for `schedulerKind`**

In `src/data/backup.test.ts`:
- `deckA` literal (line 21): add `schedulerKind: 'sm2'` → `const deckA: Deck = { id: 'a', name: 'Spanish', createdAt: 10, schedulerKind: 'sm2' }`.
- `collectBackup` expected (lines 31–37): add `schedulerKind: 'sm2',` to the deck object.
- `serializeBackup` test `decks` literal (line 48): `[{ name: 'Spanish', createdAt: 10, schedulerKind: 'sm2', cards: [] }]`.
- `parseBackup` `valid` input (line 58) and its round-trip expectation (line 64): add `schedulerKind: 'sm2',` to the deck object in both.
- Add a test that legacy decks (no `schedulerKind`) default to `'sm2'`:

```ts
  it('defaults a deck without schedulerKind to sm2', () => {
    const legacy = JSON.stringify({
      format: 'rem-backup', version: 1, exportedAt: 1,
      decks: [{ name: 'X', createdAt: 1, cards: [] }],
    })
    expect(parseBackup(legacy)[0].schedulerKind).toBe('sm2')
  })
```

- [ ] **Step 6: Update DexieStorage importDecks tests**

In `src/data/dexie/DexieStorage.test.ts`, add `schedulerKind: 'sm2'` to each `importDecks` input deck literal (the objects at lines ~108, ~133, ~151). Example for the first:

```ts
    const result = await storage.importDecks([
      {
        name: 'Spanish',
        createdAt: 5,
        schedulerKind: 'sm2',
        cards: [
          { front: 'hola', back: 'hello', createdAt: 6, updatedAt: 7, scheduling: { kind: 'sm2', repetitions: 2, intervalDays: 4, easeFactor: 2.7, due: 8 } },
        ],
      },
    ])
```

- [ ] **Step 7: Typecheck and run the full suite**

Run: `npm run typecheck && npm test`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/domain/models.ts src/data/Storage.ts src/data/dexie/DexieStorage.ts \
  src/data/backup.ts src/data/backup.test.ts src/data/dexie/DexieStorage.test.ts
git commit -m "feat: decks carry a schedulerKind (persisted + backed up)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: FSRS scheduler module

Add `ts-fsrs` and implement `FSRSScheduler` behind the `Scheduler` interface. Standalone — not wired into the app yet.

**Files:**
- Modify: `package.json` (dependency)
- Create: `src/domain/scheduler/fsrs.ts`
- Create: `src/domain/scheduler/fsrs.test.ts`

**Interfaces:**
- Consumes: `Scheduler` interface; `FSRSState`, `Grade` from Task 1.
- Produces: `class FSRSScheduler implements Scheduler` (default export-style `export class`).

- [ ] **Step 1: Install ts-fsrs**

Run: `npm install ts-fsrs`
Then confirm the Card/param field names against the installed version:
Run: `node -e "const {createEmptyCard}=require('ts-fsrs'); console.log(Object.keys(createEmptyCard(new Date())))"`
Expected: a list including `due stability difficulty reps lapses state last_review` (and `scheduled_days`, `learning_steps`, `elapsed_days`). If field names differ, adjust the mapping in Step 3 accordingly.

- [ ] **Step 2: Write the failing FSRS test**

Create `src/domain/scheduler/fsrs.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { FSRSScheduler } from './fsrs'

const now = 1_700_000_000_000
const scheduler = new FSRSScheduler()

describe('FSRSScheduler.initial', () => {
  it('starts a new card immediately due, unreviewed, kind fsrs', () => {
    const s = scheduler.initial(now)
    expect(s.kind).toBe('fsrs')
    expect(s.due).toBe(now)
    if (s.kind !== 'fsrs') throw new Error('expected fsrs')
    expect(s.reps).toBe(0)
    expect(s.lapses).toBe(0)
    expect(s.state).toBe(0)
    expect(s.lastReview).toBeNull()
  })
})

describe('FSRSScheduler.next', () => {
  it('schedules a reviewed card into the future and records the review time', () => {
    const s = scheduler.next(scheduler.initial(now), 'good', now)
    expect(s.kind).toBe('fsrs')
    if (s.kind !== 'fsrs') throw new Error('expected fsrs')
    expect(s.due).toBeGreaterThan(now)
    expect(s.reps).toBe(1)
    expect(s.lastReview).toBe(now)
  })

  it('schedules "easy" further out than "good"', () => {
    const init = scheduler.initial(now)
    const good = scheduler.next(init, 'good', now)
    const easy = scheduler.next(init, 'easy', now)
    expect(easy.due).toBeGreaterThan(good.due)
  })

  it('counts a lapse and reschedules sooner than "good" when failing a learned card', () => {
    let s = scheduler.next(scheduler.initial(now), 'good', now)
    const reviewedAt = s.due
    const again = scheduler.next(s, 'again', reviewedAt)
    const good = scheduler.next(s, 'good', reviewedAt)
    if (again.kind !== 'fsrs' || good.kind !== 'fsrs') throw new Error('expected fsrs')
    expect(again.lapses).toBe(1)
    expect(again.due).toBeLessThan(good.due)
  })

  it('is deterministic (fuzz disabled)', () => {
    const a = scheduler.next(scheduler.initial(now), 'good', now)
    const b = scheduler.next(scheduler.initial(now), 'good', now)
    expect(a).toEqual(b)
  })
})
```

- [ ] **Step 3: Implement the FSRS adapter**

Create `src/domain/scheduler/fsrs.ts`:

```ts
import { createEmptyCard, fsrs, generatorParameters, Rating, type Card as FsrsCard } from 'ts-fsrs'
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { Scheduler } from './Scheduler'

const params = generatorParameters({
  enable_fuzz: false, // deterministic, so scheduling is testable
  enable_short_term: false, // day-granular: skip sub-day learning steps
  request_retention: 0.9,
})

const RATING: Record<Grade, Rating> = {
  again: Rating.Again,
  hard: Rating.Hard,
  good: Rating.Good,
  easy: Rating.Easy,
}

/** Thin adapter over ts-fsrs, mapping its Date/Card API to our pure
 *  (state, grade, now) numeric-ms contract. Default global weights only. */
export class FSRSScheduler implements Scheduler {
  private readonly f = fsrs(params)

  initial(now: number): SchedulingState {
    return toState(createEmptyCard(new Date(now)), now)
  }

  next(state: SchedulingState, grade: Grade, now: number): SchedulingState {
    if (state.kind !== 'fsrs') throw new Error('FSRSScheduler received non-FSRS state')
    const { card } = this.f.next(toCard(state), new Date(now), RATING[grade])
    return toState(card, now)
  }
}

function toCard(s: FSRSState): FsrsCard {
  return {
    due: new Date(s.due),
    stability: s.stability,
    difficulty: s.difficulty,
    elapsed_days: 0, // recomputed by ts-fsrs from last_review + now
    scheduled_days: 0,
    learning_steps: 0,
    reps: s.reps,
    lapses: s.lapses,
    state: s.state,
    last_review: s.lastReview != null ? new Date(s.lastReview) : undefined,
  } as FsrsCard
}

function toState(card: FsrsCard, now: number): FSRSState {
  return {
    kind: 'fsrs',
    due: card.due.getTime(),
    stability: card.stability,
    difficulty: card.difficulty,
    reps: card.reps,
    lapses: card.lapses,
    state: card.state,
    lastReview: card.last_review ? card.last_review.getTime() : null,
  }
}
```

- [ ] **Step 4: Run the FSRS tests**

Run: `npx vitest run src/domain/scheduler/fsrs.test.ts`
Expected: PASS. If a field name in `toCard`/`toState` mismatches the installed ts-fsrs version (per Step 1), fix the mapping and re-run.

- [ ] **Step 5: Typecheck and commit**

Run: `npm run typecheck`
Expected: PASS.

```bash
git add package.json package-lock.json src/domain/scheduler/fsrs.ts src/domain/scheduler/fsrs.test.ts
git commit -m "feat: FSRSScheduler adapter over ts-fsrs

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: Per-deck wiring (registry + dispatch + per-deck card creation)

Replace the `scheduler` singleton with a `getScheduler(kind)` registry, make `createCard` use the deck's scheduler, and dispatch review/grade by `scheduling.kind`. After this task, creating an FSRS deck produces FSRS-scheduled cards end-to-end.

**Files:**
- Modify: `src/domain/scheduler/index.ts`
- Modify: `src/data/dexie/DexieStorage.ts`
- Modify: `src/data/StorageContext.tsx`
- Modify: `src/test/seed.ts`
- Modify: `src/features/review/GradeButtons.tsx`
- Modify: `src/features/review/ReviewPage.tsx`
- Modify: `src/data/dexie/DexieStorage.test.ts`
- Modify: `src/features/review/reviewCycle.test.ts`

**Interfaces:**
- Consumes: `SM2Scheduler` (Task 1), `FSRSScheduler` (Task 3), `Deck.schedulerKind` (Task 2).
- Produces: `function getScheduler(kind: SchedulerKind): Scheduler`; `DexieStorage` constructor is now `constructor(db: RemDB)` (no scheduler arg).

- [ ] **Step 1: Build the scheduler registry**

Replace `src/domain/scheduler/index.ts` with:

```ts
import type { SchedulerKind } from '../models'
import type { Scheduler } from './Scheduler'
import { SM2Scheduler } from './sm2'
import { FSRSScheduler } from './fsrs'

export type { Scheduler } from './Scheduler'
export { MS_PER_DAY } from './sm2'

const SCHEDULERS: Record<SchedulerKind, Scheduler> = {
  sm2: new SM2Scheduler(),
  fsrs: new FSRSScheduler(),
}

/** Resolve the scheduling algorithm for a given kind. */
export function getScheduler(kind: SchedulerKind): Scheduler {
  return SCHEDULERS[kind]
}
```

- [ ] **Step 2: Use the deck's scheduler in `createCard`; drop the injected scheduler**

In `src/data/dexie/DexieStorage.ts`:
- Change the scheduler import (line 2) to: `import { getScheduler } from '../../domain/scheduler'`.
- Change the constructor (lines 9–12) to drop the scheduler:

```ts
export class DexieStorage implements Storage {
  constructor(private readonly db: RemDB) {}
```

- Update `createCard` (lines 39–52) to resolve the deck's scheduler:

```ts
  async createCard(deckId: ID, front: string, back: string): Promise<Card> {
    const now = Date.now()
    const deck = await this.db.decks.get(deckId)
    const kind = deck?.schedulerKind ?? 'sm2'
    const card: Card = {
      id: crypto.randomUUID(),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      scheduling: getScheduler(kind).initial(now),
    }
    await this.db.cards.add(card)
    return card
  }
```

- [ ] **Step 3: Update storage construction sites**

In `src/data/StorageContext.tsx`:
- Remove the scheduler import (line 2).
- Change line 8 to: `const defaultStorage: Storage = new DexieStorage(new RemDB())`.

In `src/test/seed.ts`:
- Change the import (line 3) to: `import { MS_PER_DAY } from '../domain/scheduler'`.
- Change line 15 to: `return new DexieStorage(new RemDB(name))`.

- [ ] **Step 4: Dispatch review/grade by `scheduling.kind`**

In `src/features/review/ReviewPage.tsx`:
- Change the scheduler import (line 4) to: `import { getScheduler } from '../../domain/scheduler'`.
- Change the grade computation (line 51) to: `const next = getScheduler(current.scheduling.kind).next(current.scheduling, g, Date.now())`.

In `src/features/review/GradeButtons.tsx`:
- Change the import (added in Task 1) to: `import { getScheduler, MS_PER_DAY } from '../../domain/scheduler'`.
- Change the `grade-hint` line to resolve by kind:

```tsx
          <span className="grade-hint">
            {formatInterval(
              Math.max(1, Math.round((getScheduler(scheduling.kind).next(scheduling, grade, now).due - now) / MS_PER_DAY)),
            )}
          </span>
```

- [ ] **Step 5: Update tests that constructed DexieStorage with a scheduler**

In `src/data/dexie/DexieStorage.test.ts`:
- Change the import (line 5) to: `import { MS_PER_DAY } from '../../domain/scheduler'` (drop `SM2Scheduler`).
- Change line 14 to: `storage = new DexieStorage(db)`.

In `src/features/review/reviewCycle.test.ts`:
- Change the import (line 5) to: `import { MS_PER_DAY } from '../../domain/scheduler/sm2'` (drop `SM2Scheduler`).
- Remove `const scheduler = new SM2Scheduler()` (line 13).
- Change line 18 to: `storage = new DexieStorage(new RemDB(DB))`.
- Replace the manual `scheduler.next(...)` (line 28) with the deck's scheduler:

```ts
import { getScheduler } from '../../domain/scheduler'
```
```ts
    const next = getScheduler(card.scheduling.kind).next(card.scheduling, 'good', t0)
```

- [ ] **Step 6: Add a test that an FSRS deck creates FSRS cards**

In `src/data/dexie/DexieStorage.test.ts`, add inside `describe('cards', ...)`:

```ts
  it('creates FSRS-scheduled cards in an FSRS deck', async () => {
    const deck = await storage.createDeck('Algo', 'fsrs')
    const card = await storage.createCard(deck.id, 'q', 'a')
    expect(card.scheduling.kind).toBe('fsrs')
  })
```

- [ ] **Step 7: Add an FSRS review-cycle integration test**

In `src/features/review/reviewCycle.test.ts`, add:

```ts
  it('grades an FSRS card and pushes it out of today', async () => {
    const deck = await storage.createDeck('FSRS deck', 'fsrs')
    const card = await storage.createCard(deck.id, 'q', 'a')
    const t0 = Date.now()
    expect(await storage.countDue(deck.id, t0)).toBe(1)

    const next = getScheduler(card.scheduling.kind).next(card.scheduling, 'good', t0)
    await storage.updateCard(card.id, { scheduling: next })

    expect(await storage.countDue(deck.id, t0)).toBe(0)
  })
```

- [ ] **Step 8: Typecheck and run the full suite**

Run: `npm run typecheck && npm test`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/domain/scheduler/index.ts src/data/dexie/DexieStorage.ts src/data/StorageContext.tsx \
  src/test/seed.ts src/features/review/GradeButtons.tsx src/features/review/ReviewPage.tsx \
  src/data/dexie/DexieStorage.test.ts src/features/review/reviewCycle.test.ts
git commit -m "feat: per-deck scheduler registry + dispatch by card kind

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 5: Dexie v2 migration for legacy data

Stamp `schedulerKind`/`kind` onto records written before the discriminant existed, so data at rest always satisfies the union. New, fresh databases open directly at v2 and are unaffected.

**Files:**
- Modify: `src/data/dexie/db.ts`
- Create: `src/data/dexie/migration.test.ts`

**Interfaces:**
- Consumes: nothing new.
- Produces: `RemDB` at schema `version(2)` with a backfill upgrade.

- [ ] **Step 1: Write the failing migration test**

Create `src/data/dexie/migration.test.ts`:

```ts
import { describe, it, expect, afterEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from './db'

const NAME = 'rem-migration-test'

afterEach(async () => {
  await Dexie.delete(NAME)
})

describe('v2 migration', () => {
  it('stamps schedulerKind/kind onto legacy records', async () => {
    // Write v1-shaped data with a bare v1 Dexie instance.
    const v1 = new Dexie(NAME)
    v1.version(1).stores({ decks: 'id, createdAt', cards: 'id, deckId, createdAt' })
    await v1.open()
    await v1.table('decks').add({ id: 'd1', name: 'Old', createdAt: 1 })
    await v1.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { repetitions: 1, intervalDays: 3, easeFactor: 2.5, due: 9 },
    })
    v1.close()

    // Reopen through RemDB (declares v2) to trigger the upgrade.
    const db = new RemDB(NAME)
    const deck = await db.decks.get('d1')
    const card = await db.cards.get('c1')
    expect(deck?.schedulerKind).toBe('sm2')
    expect(card?.scheduling.kind).toBe('sm2')
    db.close()
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/data/dexie/migration.test.ts`
Expected: FAIL — `schedulerKind`/`kind` are `undefined` (no v2 upgrade yet).

- [ ] **Step 3: Add the v2 upgrade**

Replace `src/data/dexie/db.ts` with:

```ts
import Dexie, { type EntityTable } from 'dexie'
import type { Card, Deck } from '../../domain/models'

/** IndexedDB schema. Indexed fields are listed; payloads are stored whole. */
export class RemDB extends Dexie {
  decks!: EntityTable<Deck, 'id'>
  cards!: EntityTable<Card, 'id'>

  constructor(name = 'rem') {
    super(name)
    this.version(1).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
    })
    // v2: stamp the scheduling-algorithm discriminant onto pre-existing
    // records written before per-deck schedulers existed. Schema unchanged.
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
  }
}
```

- [ ] **Step 4: Run the migration test**

Run: `npx vitest run src/data/dexie/migration.test.ts`
Expected: PASS.

- [ ] **Step 5: Typecheck, full suite, commit**

Run: `npm run typecheck && npm test`
Expected: PASS.

```bash
git add src/data/dexie/db.ts src/data/dexie/migration.test.ts
git commit -m "feat: Dexie v2 migration stamps scheduler discriminant on legacy data

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 6: UI — per-deck algorithm picker + deck badge

Let users choose an algorithm when creating a deck (default FSRS), and show the deck's algorithm on its detail page.

**Files:**
- Modify: `src/features/decks/DeckListPage.tsx`
- Modify: `src/features/cards/DeckDetailPage.tsx`
- Modify: `src/ui/styles.css`
- Create: `src/features/decks/DeckListPage.browser.test.tsx`

**Interfaces:**
- Consumes: `Storage.createDeck(name, kind)`, `SchedulerKind`, `Deck.schedulerKind`.

- [ ] **Step 1: Write the failing picker test**

Create `src/features/decks/DeckListPage.browser.test.tsx`:

```tsx
import { test, expect } from 'vitest'
import { page } from 'vitest/browser'
import { DeckListPage } from './DeckListPage'
import { freshStorage } from '../../test/seed'
import { renderRoute } from '../../test/renderRoute'

test('creates a deck with the chosen scheduler', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  // Default selection is FSRS.
  await page.getByLabelText('New deck name').fill('Algorithms')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('fsrs')
})

test('creates an SM-2 deck when SM-2 is selected', async () => {
  const storage = freshStorage()
  await renderRoute({ storage, entry: '/', path: '/', element: <DeckListPage /> })

  await page.getByLabelText('New deck name').fill('Spanish')
  await page.getByLabelText('Scheduler').selectOptions('sm2')
  await page.getByRole('button', { name: 'Add deck' }).click()

  await expect.poll(async () => (await storage.listDecks())[0]?.schedulerKind).toBe('sm2')
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run src/features/decks/DeckListPage.browser.test.tsx`
Expected: FAIL — no "Scheduler" control; created deck is not FSRS.

- [ ] **Step 3: Add the picker to the create form**

In `src/features/decks/DeckListPage.tsx`:
- Add `SchedulerKind` import: `import type { SchedulerKind } from '../../domain/models'`.
- Add state next to `name` (after line 15): `const [kind, setKind] = useState<SchedulerKind>('fsrs')`.
- In `addDeck`, pass the kind (line 33): `await storage.createDeck(trimmed, kind)`.
- In the form (between the input and the Add button, after line 74), add the select:

```tsx
        <select
          className="text-input sched-picker"
          value={kind}
          onChange={(e) => setKind(e.target.value as SchedulerKind)}
          aria-label="Scheduler"
        >
          <option value="fsrs">FSRS (recommended)</option>
          <option value="sm2">SM-2</option>
        </select>
```

- [ ] **Step 4: Run the picker test**

Run: `npx vitest run src/features/decks/DeckListPage.browser.test.tsx`
Expected: PASS.

- [ ] **Step 5: Show the algorithm on the deck detail header**

In `src/features/cards/DeckDetailPage.tsx`, add a badge beside the deck title. Replace the title line (line 47) inside `<div className="row between">`:

```tsx
        <h1 className="page-title">
          {deck.name}
          <span className="sched-badge">{deck.schedulerKind === 'fsrs' ? 'FSRS' : 'SM-2'}</span>
        </h1>
```

- [ ] **Step 6: Add styles for the picker and badge**

In `src/ui/styles.css`, append:

```css
.sched-picker {
  flex: 0 0 auto;
}

.sched-badge {
  margin-left: var(--space-2);
  padding: 0.1rem 0.45rem;
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.03em;
  color: var(--muted);
  background: var(--surface-inset);
  border: 1px solid var(--border);
  border-radius: 999px;
  vertical-align: middle;
}
```

(Tokens `--space-2`, `--text-xs`, `--muted`, `--surface-inset`, `--border` are all defined in `src/ui/tokens.css`.)

- [ ] **Step 7: Typecheck and run the full suite**

Run: `npm run typecheck && npm test`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/features/decks/DeckListPage.tsx src/features/decks/DeckListPage.browser.test.tsx \
  src/features/cards/DeckDetailPage.tsx src/ui/styles.css
git commit -m "feat: per-deck scheduler picker on create + algorithm badge

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 7: Mark roadmap item shipped

- [ ] **Step 1: Update the roadmap**

In `docs/ROADMAP.md`, change the Mid-term item 4 line (line 96) to mark FSRS shipped, matching the style of the shipped near-term items:

```markdown
4. ✅ **FSRS scheduler** (#3) — **shipped**: `ts-fsrs` behind the existing `Scheduler`
   interface, chosen **per deck** at creation (new decks default to FSRS; existing
   decks stay SM-2). `SchedulingState` is now a discriminated union; a Dexie v2
   migration stamps legacy records; backup round-trips the algorithm.
```

- [ ] **Step 2: Commit**

```bash
git add docs/ROADMAP.md
git commit -m "docs: mark FSRS scheduler shipped in roadmap

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-review notes

- **Spec coverage:** §1 model → Task 1 (+ Deck field Task 2); §2 scheduler module/registry → Tasks 3–4; §3 storage + migration → Tasks 2, 4, 5; §4 consumers → Tasks 1, 4; §5 backup → Tasks 1–2; §6 UI → Task 6; §7 dependency → Task 3; §8 testing → tests in every task. Roadmap update → Task 7.
- **Ordering rationale:** the discriminated union (Task 1) must precede `FSRSState`-producing code; `Deck.schedulerKind` (Task 2) must precede per-deck `createCard` (Task 4); the registry (Task 4) needs both schedulers (Tasks 1, 3). Migration (Task 5) is independent of new-data paths and can land after wiring. UI (Task 6) is last.
- **Type consistency:** `SchedulerKind`, `SM2State`/`FSRSState` field names (`reps`, `lapses`, `state`, `lastReview`, `stability`, `difficulty`), `getScheduler(kind)`, `createDeck(name, kind?)`, `DeckBackup.schedulerKind` are used identically across tasks.
- **Known interim:** ts-fsrs `Card` field names are verified at install (Task 3 Step 1); adjust the mapping if the installed version differs.
