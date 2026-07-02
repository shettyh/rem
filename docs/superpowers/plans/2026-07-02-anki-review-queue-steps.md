# Anki Review-Queue Steps (sub-project #3a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make grading behave like Anki — new cards walk fixed learning steps and lapsed cards walk relearning steps before FSRS schedules them long-term — while each deck's real FSRS params drive the math and new cards enter in the deck's insertion order.

**Architecture:** Classic fixed steps run in TypeScript (Approach A); the existing Rust `fsrs_next_states` command is untouched and consulted only at graduation and on review-state grades. A new step machine (`reviewScheduler.nextStates`) wraps the scheduler seam; a new `ReviewSession` model gives `ReviewPage` a dynamic in-session queue with learn-ahead re-insertion.

**Tech Stack:** TypeScript, React, Dexie (IndexedDB), Vitest (unit, jsdom) + Vitest browser (Playwright/chromium), Tauri (fsrs-rs, unchanged here).

**Spec:** `docs/superpowers/specs/2026-07-02-anki-review-queue-steps-design.md`

## Global Constraints

- **FSRS picks every long-term interval; deck min/max only clamp it.** Graduation due = FSRS's own good/easy interval; relearning-graduation clamps FSRS good/easy to `[minimumInterval, maximumInterval]` days.
- **`reps` counts FSRS reviews only.** Learning-step grades do not increment `reps`, do not change `stability`/`difficulty`/`lapses`, and do not feed FSRS. Graduation seeds FSRS on the `reps === 0 → current = None` path.
- **`isNew(s)` = `s.state === 0`** (not `reps === 0`).
- **`Hard` on a step repeats the current step** (does not advance, does not average).
- **Learn-ahead window is a fixed `20 * 60_000` ms constant** (`LEARN_AHEAD_MS`), not a deck setting.
- **`insertionOrder` applies to single-deck review only**; the cross-deck ("All decks") path keeps its existing whole-queue `shuffle`.
- **The Rust side is not modified.** The extra `step` field on the IPC payload is ignored by serde.
- **Field-carry rule:** a pure step transition starts from the prior `scheduling`, changes only `state`, `step`, `due`, sets `lastReview = now`, and leaves `reps`, `stability`, `difficulty`, `lapses`, `kind` unchanged.
- **Branch:** `feat/review-queue-steps`. **Commits authored** `shettyh <manjunathshetty@live.com>` with trailer `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **Never stage `features.md`** (pre-existing untracked backlog, not part of this work).
- **Verify commands:** unit test file — `npx vitest run --project unit <path>`; browser test — `npx vitest run --project browser <path>`; whole suite — `npm test`; types — `npm run typecheck`.

---

### Task 1: Shared step tokenizer + duration parser

**Files:**
- Create: `src/domain/scheduler/steps.ts`
- Create: `src/domain/scheduler/steps.test.ts`
- Modify: `src/features/decks/DeckSettingsPage.tsx:13-16` (remove local `parseSteps`, import from new module)
- Modify: `src/features/decks/deckSettings.test.ts:2` (repoint `parseSteps` import; add `parseStepsMs` cases)

**Interfaces:**
- Produces: `parseSteps(raw: string): string[]`, `parseStepsMs(raw: string): number[]` (both from `src/domain/scheduler/steps.ts`).

- [ ] **Step 1: Write the failing test**

Create `src/domain/scheduler/steps.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { parseSteps, parseStepsMs } from './steps'

describe('parseSteps', () => {
  it('splits on whitespace', () => {
    expect(parseSteps('1m 10m 1d')).toEqual(['1m', '10m', '1d'])
  })
  it('trims and drops blanks', () => {
    expect(parseSteps('  10m   1d ')).toEqual(['10m', '1d'])
  })
  it('empty string → []', () => {
    expect(parseSteps('   ')).toEqual([])
  })
})

describe('parseStepsMs', () => {
  it('parses s/m/h/d units', () => {
    expect(parseStepsMs('30s 10m 1h 1d')).toEqual([30_000, 600_000, 3_600_000, 86_400_000])
  })
  it('treats a bare integer as minutes', () => {
    expect(parseStepsMs('1 10')).toEqual([60_000, 600_000])
  })
  it('drops unparseable tokens', () => {
    expect(parseStepsMs('10m foo 1d')).toEqual([600_000, 86_400_000])
  })
  it('empty string → []', () => {
    expect(parseStepsMs('')).toEqual([])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --project unit src/domain/scheduler/steps.test.ts`
Expected: FAIL — `Failed to resolve import "./steps"`.

- [ ] **Step 3: Write minimal implementation**

Create `src/domain/scheduler/steps.ts`:

```ts
/** Split a steps string into chip tokens, e.g. "1m 10m 1d" → ["1m","10m","1d"]. */
export function parseSteps(raw: string): string[] {
  return raw.split(/\s+/).filter(Boolean)
}

const UNIT_MS: Record<string, number> = { s: 1000, m: 60_000, h: 3_600_000, d: 86_400_000 }

/** One token → milliseconds, or null if unparseable. Units s/m/h/d; a bare
 *  integer means minutes (Anki convention). */
function stepMs(token: string): number | null {
  const m = /^(\d+)([smhd]?)$/.exec(token.trim())
  if (!m) return null
  return Number(m[1]) * UNIT_MS[m[2] || 'm']
}

/** A steps string → milliseconds list, dropping unparseable tokens. */
export function parseStepsMs(raw: string): number[] {
  return parseSteps(raw)
    .map(stepMs)
    .filter((n): n is number => n !== null)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run --project unit src/domain/scheduler/steps.test.ts`
Expected: PASS (7 tests).

- [ ] **Step 5: Repoint `DeckSettingsPage` and its test to the shared module**

In `src/features/decks/DeckSettingsPage.tsx`, delete the local definition (lines 13-16):

```ts
/** Split a space-separated steps string into chip tokens. */
export function parseSteps(raw: string): string[] {
  return raw.split(/\s+/).filter(Boolean)
}
```

and add to the imports block near the top (after the `deckColor` import):

```ts
import { parseSteps } from '../../domain/scheduler/steps'
```

In `src/features/decks/deckSettings.test.ts`, change line 2 from:

```ts
import { parseSteps } from './DeckSettingsPage'
```

to:

```ts
import { parseSteps } from '../../domain/scheduler/steps'
```

- [ ] **Step 6: Run the affected tests + typecheck**

Run: `npx vitest run --project unit src/domain/scheduler/steps.test.ts src/features/decks/deckSettings.test.ts && npm run typecheck`
Expected: PASS, and typecheck clean.

- [ ] **Step 7: Commit**

```bash
git add src/domain/scheduler/steps.ts src/domain/scheduler/steps.test.ts src/features/decks/DeckSettingsPage.tsx src/features/decks/deckSettings.test.ts
git commit -m "feat(scheduler): shared step tokenizer + duration parser

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Drop `graduatingInterval` and `easyInterval`

**Files:**
- Modify: `src/domain/models.ts:24-25` (interface) and `:41-42` (defaults)
- Modify: `src/features/decks/DeckSettingsPage.tsx:167-182` (remove two `Stepper` rows)

**Interfaces:**
- Produces: `DeckSettings` without `graduatingInterval`/`easyInterval`; `DEFAULT_DECK_SETTINGS` without them.

- [ ] **Step 1: Remove the two fields from the model**

In `src/domain/models.ts`, delete these two lines from the `DeckSettings` interface:

```ts
  graduatingInterval: number
  easyInterval: number
```

and delete these two lines from `DEFAULT_DECK_SETTINGS`:

```ts
  graduatingInterval: 1,
  easyInterval: 4,
```

- [ ] **Step 2: Remove the two UI controls**

In `src/features/decks/DeckSettingsPage.tsx`, delete lines 167-182 — the Graduating-interval row, the Easy-interval row, and the two `ds-rule` dividers between/after them — leaving the `ds-rule` at line 166 so a single divider separates the learning-steps chips from Insertion order:

```tsx
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
                <div className="ds-row-sub">Days when answering a new card "Easy".</div>
              </div>
              <Stepper value={settings.easyInterval} onChange={(v) => set('easyInterval', v)} label="Easy interval" step={1} min={1} max={365} format={(v) => `${v}d`} />
            </div>
            <div className="ds-rule" />
```

(The remaining block immediately after is the `<div className="ds-row">` for **Insertion order**.)

- [ ] **Step 3: Typecheck to prove nothing else referenced them**

Run: `npm run typecheck`
Expected: PASS. (No task consumes these fields; the step machine uses FSRS intervals instead.)

- [ ] **Step 4: Confirm removal + run deck tests**

Run: `grep -rn "graduatingInterval\|easyInterval" src ; npx vitest run --project browser src/features/decks/DeckSettingsPage.browser.test.tsx`
Expected: `grep` prints nothing; browser test PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/models.ts src/features/decks/DeckSettingsPage.tsx
git commit -m "feat(decks): drop graduating/easy interval (FSRS picks intervals now)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Add `FSRSState.step` + Dexie v7 backfill

**Files:**
- Modify: `src/domain/models.ts` (`FSRSState` gains `step`)
- Modify: `src/domain/scheduler/tauriFsrs.ts:37,47` (`toState` + `initial` set `step`)
- Modify: `src/domain/scheduler/fakeScheduler.ts:8,20-29` (`emptyCard` + `make` set `step`)
- Modify: `src/data/dexie/db.ts` (add `version(7)` backfill)
- Modify: `src/data/dexie/migration.test.ts` (add a v7 test)
- Modify: `src/domain/scheduler/tauriFsrs.test.ts:9` + `src/domain/scheduler/fakeScheduler.test.ts:11` (expected objects gain `step: 0`)
- Modify: the ~14 remaining `FSRSState` literal fixtures across test files (mechanical `step: 0`)

**Interfaces:**
- Produces: `FSRSState.step: number` (0 when `state ∈ {0,2}`), present on every constructed/stored `FSRSState`.

- [ ] **Step 1: Write the failing migration test**

In `src/data/dexie/migration.test.ts`, add this `describe` block (after the existing v6 block):

```ts
describe('learning-step migration (v7)', () => {
  it('backfills step: 0 on pre-v7 card scheduling', async () => {
    const v6 = new Dexie(NAME)
    v6.version(6).stores({
      decks: 'id, createdAt',
      cards: 'id, deckId, createdAt',
      tombstones: 'id, deletedAt',
      assets: 'hash',
    })
    await v6.open()
    await v6.table('cards').add({
      id: 'c1', deckId: 'd1', front: 'q', back: 'a', createdAt: 1, updatedAt: 1,
      scheduling: { kind: 'fsrs', stability: 5, difficulty: 5, reps: 3, lapses: 1, state: 2, lastReview: 100, due: 200 },
    })
    v6.close()

    const db = new RemDB(NAME)
    const card = await db.cards.get('c1')
    expect(card?.scheduling.step).toBe(0)
    db.close()
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run --project unit src/data/dexie/migration.test.ts`
Expected: FAIL — `expected undefined to be 0` (no v7 migration yet).

- [ ] **Step 3: Add `step` to the model**

In `src/domain/models.ts`, add to `FSRSState`, immediately after the `state` field:

```ts
  /** Index into the deck's learn/relearn steps; 0 when state ∈ {0 New, 2 Review}. */
  step: number
```

- [ ] **Step 4: Set `step` in every constructor**

In `src/domain/scheduler/tauriFsrs.ts`, change `toState` (line 37):

```ts
function toState(dto: FsrsStateDto): FSRSState {
  return { kind: 'fsrs', step: 0, ...dto }
}
```

and `initial` (line 47):

```ts
  initial(now: number): SchedulingState {
    return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now }
  }
```

In `src/domain/scheduler/fakeScheduler.ts`, change `emptyCard` (line 7-9):

```ts
function emptyCard(now: number): FSRSState {
  return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now }
}
```

and add `step: 0` to the `make` object (inside `previewNextStates`, after `state: 2,`):

```ts
      state: 2,
      step: 0,
```

- [ ] **Step 5: Add the v7 migration**

In `src/data/dexie/db.ts`, append after the `version(6)` block (inside the constructor):

```ts
    // v7: learning/relearning steps. Backfill step: 0 on cards scheduled before
    // the step machine existed. Schema unchanged.
    this.version(7)
      .stores({
        decks: 'id, createdAt',
        cards: 'id, deckId, createdAt',
        tombstones: 'id, deletedAt',
        assets: 'hash',
      })
      .upgrade(async (tx) => {
        await tx.table('cards').toCollection().modify((c) => {
          if (c.scheduling && c.scheduling.step === undefined) c.scheduling.step = 0
        })
      })
```

- [ ] **Step 6: Update the two scheduler-test expectations**

In `src/domain/scheduler/tauriFsrs.test.ts:9`, change to:

```ts
    expect(out.good).toEqual({ kind: 'fsrs', step: 0, ...branch })
```

In `src/domain/scheduler/fakeScheduler.test.ts:11`, change the expected object to include `step: 0`:

```ts
    expect(c).toEqual({ kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now })
```

- [ ] **Step 7: Add `step: 0` to the remaining FSRSState fixtures**

TypeScript now flags every `FSRSState` object literal missing `step`. Find them and insert `step: 0` (place it right after the `state:` value in each literal):

Run: `grep -rn "kind: 'fsrs'" src`

Add `step: 0` to each literal in these files (all are `state: 0` or `state: 2`, so `step: 0` is correct): `src/features/settings/SettingsPage.browser.test.tsx`, `src/features/cards/cardStatus.test.ts`, `src/features/review/dueOverview.test.ts`, `src/data/backup.test.ts`, `src/data/dexie/DexieStorage.test.ts`, `src/data/sync/merge.test.ts`, `src/data/sync/snapshot.test.ts`, `src/data/dexie/migration.test.ts` (the v5-block fixture). The gate is typecheck (next step) — every remaining literal must compile.

- [ ] **Step 8: Typecheck, then run the full suite**

Run: `npm run typecheck && npm test`
Expected: typecheck clean; all tests PASS (including the new v7 migration test).

- [ ] **Step 9: Commit**

```bash
git add src/domain/models.ts src/domain/scheduler/tauriFsrs.ts src/domain/scheduler/fakeScheduler.ts src/data/dexie/db.ts src/data/dexie/migration.test.ts src/domain/scheduler/tauriFsrs.test.ts src/domain/scheduler/fakeScheduler.test.ts src/features src/data
git commit -m "feat(scheduler): add FSRSState.step with Dexie v7 backfill

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Thread deck params through the scheduler seam

**Files:**
- Modify: `src/domain/scheduler/Scheduler.ts` (move `DeckFsrsParams` here; add `params` to signature)
- Modify: `src/domain/scheduler/tauriFsrs.ts` (import `DeckFsrsParams`; drop `DEFAULT_DECK_FSRS_PARAMS`; forward `params`)
- Modify: `src/domain/scheduler/fakeScheduler.ts` (accept + ignore `params`)
- Create: `src/domain/scheduler/reviewScheduler.ts` (with `settingsToParams` only for now)
- Create: `src/domain/scheduler/reviewScheduler.test.ts` (`settingsToParams` case)
- Modify: `src/domain/scheduler/fakeScheduler.test.ts` (3-arg calls)
- Modify: `src/features/review/reviewCycle.test.ts:27` (3-arg call)
- Modify: `src/features/review/ReviewPage.tsx` (resolve per-card settings; pass `settingsToParams(...)`)

**Interfaces:**
- Consumes: `FSRSState.step` (Task 3).
- Produces: `Scheduler.previewNextStates(state, params: DeckFsrsParams, now): Promise<Record<Grade, SchedulingState>>`; `DeckFsrsParams { desiredRetention: number; maximumInterval: number; weights: number[] | null }` (from `./Scheduler`); `settingsToParams(s: DeckSettings): DeckFsrsParams` (from `./reviewScheduler`).

- [ ] **Step 1: Write the failing `settingsToParams` test**

Create `src/domain/scheduler/reviewScheduler.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { DEFAULT_DECK_SETTINGS } from '../models'
import { settingsToParams } from './reviewScheduler'

describe('settingsToParams', () => {
  it('maps deck settings to FSRS params with null weights', () => {
    const s = { ...DEFAULT_DECK_SETTINGS, desiredRetention: 0.85, maximumInterval: 1000 }
    expect(settingsToParams(s)).toEqual({ desiredRetention: 0.85, maximumInterval: 1000, weights: null })
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `npx vitest run --project unit src/domain/scheduler/reviewScheduler.test.ts`
Expected: FAIL — `Failed to resolve import "./reviewScheduler"`.

- [ ] **Step 3: Move `DeckFsrsParams` to the seam and add the `params` argument**

In `src/domain/scheduler/Scheduler.ts`, add the type and update the interface:

```ts
import type { Grade, SchedulingState } from '../models'

export interface DeckFsrsParams {
  desiredRetention: number
  maximumInterval: number
  weights: number[] | null
}

export interface Scheduler {
  /** Scheduling state for a brand-new card (immediately due). */
  initial(now: number): SchedulingState
  /** All four grade outcomes for the next review. */
  previewNextStates(
    state: SchedulingState,
    params: DeckFsrsParams,
    now: number,
  ): Promise<Record<Grade, SchedulingState>>
}
```

- [ ] **Step 4: Update `TauriFsrsScheduler`**

In `src/domain/scheduler/tauriFsrs.ts`: delete the local `DeckFsrsParams` interface (lines 5-9) and the `DEFAULT_DECK_FSRS_PARAMS` constant (lines 11-17); import the type from `./Scheduler`; forward `params`:

```ts
import { invoke } from '@tauri-apps/api/core'
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { DeckFsrsParams, Scheduler } from './Scheduler'
```

```ts
  async previewNextStates(
    state: SchedulingState,
    params: DeckFsrsParams,
    now: number,
  ): Promise<Record<Grade, SchedulingState>> {
    const dto = await invoke<NextStatesDto>('fsrs_next_states', { state, now, params })
    return mapNextStates(dto)
  }
```

(Keep `FsrsStateDto`, `NextStatesDto`, `toState`, `mapNextStates` as-is.)

- [ ] **Step 5: Update `FakeScheduler`**

In `src/domain/scheduler/fakeScheduler.ts`, import the type and accept an ignored `params`:

```ts
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { DeckFsrsParams, Scheduler } from './Scheduler'
```

```ts
  async previewNextStates(
    state: SchedulingState,
    _params: DeckFsrsParams,
    now: number,
  ): Promise<Record<Grade, SchedulingState>> {
```

(Body unchanged.)

- [ ] **Step 6: Add `settingsToParams` to a new `reviewScheduler.ts`**

Create `src/domain/scheduler/reviewScheduler.ts`:

```ts
import type { DeckSettings } from '../models'
import type { DeckFsrsParams } from './Scheduler'

/** Per-deck FSRS params from the deck's settings. Weights stay null until #5. */
export function settingsToParams(s: DeckSettings): DeckFsrsParams {
  return { desiredRetention: s.desiredRetention, maximumInterval: s.maximumInterval, weights: null }
}
```

- [ ] **Step 7: Fix the direct seam callers in tests**

In `src/domain/scheduler/fakeScheduler.test.ts`, add a params const near the top of the `describe` and pass it to every `previewNextStates` call:

```ts
const PARAMS = { desiredRetention: 0.9, maximumInterval: 36500, weights: null }
```

Update each call (lines 17, 26, 28, 33, 34) from `previewNextStates(X, now)` to `previewNextStates(X, PARAMS, now)`.

In `src/features/review/reviewCycle.test.ts`, add the import and update line 27:

```ts
import { settingsToParams } from '../../domain/scheduler/reviewScheduler'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
```

```ts
    const nexts = await getScheduler().previewNextStates(card.scheduling, settingsToParams(DEFAULT_DECK_SETTINGS), t0)
```

- [ ] **Step 8: Thread real per-card params through `ReviewPage`**

In `src/features/review/ReviewPage.tsx`, add imports:

```ts
import { settingsToParams } from '../../domain/scheduler/reviewScheduler'
import type { DeckSettings } from '../../domain/models'
```

Load a `deckId → settings` map with a live query (add near the existing `deck` query):

```ts
  const decks = useLiveQuery(() => storage.listDecks(), [])
  const settingsById = new Map<string, DeckSettings>((decks ?? []).map((d) => [d.id, d.settings]))
```

Change `fetchNexts` to build params from the current card's deck settings:

```ts
  const fetchNexts = useCallback(
    (scheduling: Card['scheduling'], deckIdOfCard: string, now: number) => {
      setSchedError(false)
      setNexts(null)
      const settings = settingsById.get(deckIdOfCard)
      if (!settings) { setSchedError(true); return }
      void getScheduler()
        .previewNextStates(scheduling, settingsToParams(settings), now)
        .then(setNexts)
        .catch((err: unknown) => {
          console.error('previewNextStates failed', err)
          setSchedError(true)
        })
    },
    [settingsById],
  )
```

Update the two `fetchNexts(...)` call sites to pass the card's deckId:

```ts
    fetchNexts(current.scheduling, current.deckId, now)          // inside reveal()
```
```ts
                  onClick={() => fetchNexts(current.scheduling, current.deckId, revealedAt)}  // Retry button
```

(The static `queue`/`index` mechanics are unchanged in this task.)

- [ ] **Step 9: Typecheck + run the affected + full suite**

Run: `npm run typecheck && npm test`
Expected: typecheck clean; all PASS. `grep -rn "DEFAULT_DECK_FSRS_PARAMS" src` prints nothing.

- [ ] **Step 10: Commit**

```bash
git add src/domain/scheduler/Scheduler.ts src/domain/scheduler/tauriFsrs.ts src/domain/scheduler/fakeScheduler.ts src/domain/scheduler/reviewScheduler.ts src/domain/scheduler/reviewScheduler.test.ts src/domain/scheduler/fakeScheduler.test.ts src/features/review/reviewCycle.test.ts src/features/review/ReviewPage.tsx
git commit -m "feat(scheduler): thread per-deck FSRS params through the seam

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: The step machine — `nextStates`

**Files:**
- Modify: `src/domain/scheduler/reviewScheduler.ts` (add `nextStates`)
- Modify: `src/domain/scheduler/reviewScheduler.test.ts` (step-machine cases)

**Interfaces:**
- Consumes: `settingsToParams` (Task 4), `parseStepsMs` (Task 1), `getScheduler` + `MS_PER_DAY` (`./index`), the seam signature (Task 4).
- Produces: `nextStates(scheduling: FSRSState, settings: DeckSettings, now: number): Promise<Record<Grade, FSRSState>>`.

- [ ] **Step 1: Write the failing tests**

Add to `src/domain/scheduler/reviewScheduler.test.ts` (the fake scheduler is active outside Tauri, so its fixed offsets drive the FSRS branches deterministically — `again +0d`, `hard +1d`, `good +3d`, `easy +7d`, `reps+1`, `lapses+1` on a review `again`):

```ts
import { nextStates } from './reviewScheduler'
import type { FSRSState } from '../models'

const DAY = 86_400_000
const S = { ...DEFAULT_DECK_SETTINGS, learnSteps: '1m 10m', relearnSteps: '10m', minimumInterval: 3, maximumInterval: 36500 }
function newCard(now: number): FSRSState {
  return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, step: 0, lastReview: null, due: now }
}

describe('nextStates — learning', () => {
  it('new card: again → step 0 in 1m, good → step 1 in 10m (no FSRS, reps stays 0)', async () => {
    const now = 1_000_000
    const ns = await nextStates(newCard(now), S, now)
    expect(ns.again).toMatchObject({ state: 1, step: 0, due: now + 60_000, reps: 0 })
    expect(ns.good).toMatchObject({ state: 1, step: 1, due: now + 600_000, reps: 0 })
  })
  it('hard repeats the current step', async () => {
    const now = 1_000_000
    const ns = await nextStates({ ...newCard(now), state: 1, step: 1 }, S, now)
    expect(ns.hard).toMatchObject({ state: 1, step: 1, due: now + 600_000 })
  })
  it('good on the last step graduates via FSRS (state 2, reps 1)', async () => {
    const now = 1_000_000
    const ns = await nextStates({ ...newCard(now), state: 1, step: 1 }, S, now)
    expect(ns.good).toMatchObject({ state: 2, step: 0, reps: 1 })
    expect(ns.good.due).toBeGreaterThan(now + DAY) // FSRS interval, not a step
  })
  it('easy graduates immediately via FSRS', async () => {
    const now = 1_000_000
    const ns = await nextStates(newCard(now), S, now)
    expect(ns.easy).toMatchObject({ state: 2, step: 0, reps: 1 })
  })
  it('empty learn steps → straight to FSRS review on good', async () => {
    const now = 1_000_000
    const ns = await nextStates(newCard(now), { ...S, learnSteps: '' }, now)
    expect(ns.good).toMatchObject({ state: 2, step: 0, reps: 1 })
  })
})

describe('nextStates — review lapse + relearning', () => {
  const review: FSRSState = { kind: 'fsrs', stability: 10, difficulty: 5, reps: 4, lapses: 0, state: 2, step: 0, lastReview: 0, due: 0 }
  it('again on a review card records a lapse and enters relearning', async () => {
    const now = 5_000_000
    const ns = await nextStates(review, S, now)
    expect(ns.again).toMatchObject({ state: 3, step: 0, due: now + 600_000, lapses: 1 })
  })
  it('good/easy stay in review (FSRS long-term)', async () => {
    const now = 5_000_000
    const ns = await nextStates(review, S, now)
    expect(ns.good).toMatchObject({ state: 2, step: 0 })
  })
  it('relearning good on the last step graduates, clamped to minimumInterval', async () => {
    const now = 5_000_000
    const relearn: FSRSState = { ...review, state: 3, step: 0 }
    const ns = await nextStates(relearn, S, now) // relearnSteps '10m' → single step, so good graduates
    expect(ns.good).toMatchObject({ state: 2, step: 0 })
    expect(ns.good.due).toBe(now + 3 * DAY) // fake good = +3d, minimumInterval 3 → clamp keeps 3d
  })
  it('relearning good graduation respects a higher minimumInterval', async () => {
    const now = 5_000_000
    const relearn: FSRSState = { ...review, state: 3, step: 0 }
    const ns = await nextStates(relearn, { ...S, minimumInterval: 5 }, now)
    expect(ns.good.due).toBe(now + 5 * DAY) // fake good = +3d, clamped up to 5d
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run --project unit src/domain/scheduler/reviewScheduler.test.ts`
Expected: FAIL — `nextStates is not a function` / import error.

- [ ] **Step 3: Implement `nextStates`**

First replace the import block at the top of `src/domain/scheduler/reviewScheduler.ts` (from Task 4) with this complete set — it adds `FSRSState`/`Grade`, `getScheduler`/`MS_PER_DAY`, and `parseStepsMs` while keeping the two Task-4 imports (no duplicates):

```ts
import type { DeckSettings, FSRSState, Grade } from '../models'
import type { DeckFsrsParams } from './Scheduler'
import { getScheduler, MS_PER_DAY } from './index'
import { parseStepsMs } from './steps'
```

Then append below the existing `settingsToParams`:

```ts
/** A pure learning/relearning step transition: keep memory, move state/step/due. */
function stepTo(base: FSRSState, state: number, step: number, dueMs: number, now: number): FSRSState {
  return { ...base, state, step, due: now + dueMs, lastReview: now }
}

/** Clamp an FSRS due date to [minDays, maxDays] whole days from now. */
function clampDays(state: FSRSState, now: number, minDays: number, maxDays: number): FSRSState {
  const days = Math.min(Math.max(Math.round((state.due - now) / MS_PER_DAY), minDays), maxDays)
  return { ...state, due: now + days * MS_PER_DAY }
}

/**
 * All four grade outcomes for a card, honouring classic learning/relearning
 * steps in TS and delegating long-term intervals to FSRS (via the seam).
 */
export async function nextStates(
  scheduling: FSRSState,
  settings: DeckSettings,
  now: number,
): Promise<Record<Grade, FSRSState>> {
  const fsrs = await getScheduler().previewNextStates(scheduling, settingsToParams(settings), now)
  const i = scheduling.step
  const min = settings.minimumInterval
  const max = settings.maximumInterval

  // New / Learning
  if (scheduling.state === 0 || scheduling.state === 1) {
    const L = parseStepsMs(settings.learnSteps)
    if (L.length === 0) return fsrs as Record<Grade, FSRSState>
    return {
      again: stepTo(scheduling, 1, 0, L[0], now),
      hard: stepTo(scheduling, 1, i, L[Math.min(i, L.length - 1)], now),
      good: i + 1 < L.length ? stepTo(scheduling, 1, i + 1, L[i + 1], now) : (fsrs.good as FSRSState),
      easy: fsrs.easy as FSRSState,
    }
  }

  // Relearning
  if (scheduling.state === 3) {
    const R = parseStepsMs(settings.relearnSteps)
    if (R.length === 0) {
      return {
        again: clampDays(fsrs.again as FSRSState, now, min, max),
        hard: clampDays(fsrs.hard as FSRSState, now, min, max),
        good: clampDays(fsrs.good as FSRSState, now, min, max),
        easy: clampDays(fsrs.easy as FSRSState, now, min, max),
      }
    }
    return {
      again: stepTo(scheduling, 3, 0, R[0], now),
      hard: stepTo(scheduling, 3, i, R[Math.min(i, R.length - 1)], now),
      good: i + 1 < R.length ? stepTo(scheduling, 3, i + 1, R[i + 1], now) : clampDays(fsrs.good as FSRSState, now, min, max),
      easy: clampDays(fsrs.easy as FSRSState, now, min, max),
    }
  }

  // Review (state 2)
  const R = parseStepsMs(settings.relearnSteps)
  const againBase = fsrs.again as FSRSState // memory updated + lapses++ by the FSRS seam
  const again = R.length === 0
    ? clampDays(againBase, now, min, max)
    : { ...againBase, state: 3, step: 0, due: now + R[0], lastReview: now }
  return { again, hard: fsrs.hard as FSRSState, good: fsrs.good as FSRSState, easy: fsrs.easy as FSRSState }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run --project unit src/domain/scheduler/reviewScheduler.test.ts`
Expected: PASS (all describes).

- [ ] **Step 5: Typecheck**

Run: `npm run typecheck`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/domain/scheduler/reviewScheduler.ts src/domain/scheduler/reviewScheduler.test.ts
git commit -m "feat(scheduler): learning/relearning step machine (nextStates)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: Dynamic session queue — `ReviewSession`

**Files:**
- Create: `src/features/review/session.ts`
- Create: `src/features/review/session.test.ts`

**Interfaces:**
- Consumes: `shuffle` (`./dueOverview`), `Card`/`DeckSettings`/`FSRSState`/`InsertionOrder` (`../../domain/models`).
- Produces: `LEARN_AHEAD_MS`, `interface SessionCard { card: Card; settings: DeckSettings }`, `buildSessionCards(cards: SessionCard[], order: InsertionOrder): SessionCard[]`, `class ReviewSession { constructor(cards: SessionCard[]); next(now: number): SessionCard | null; grade(now: number, next: FSRSState): void; get remaining(): number; get reviewed(): number }`.

- [ ] **Step 1: Write the failing tests**

Create `src/features/review/session.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import type { Card, DeckSettings, FSRSState } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { ReviewSession, buildSessionCards, LEARN_AHEAD_MS, type SessionCard } from './session'

const S: DeckSettings = DEFAULT_DECK_SETTINGS
function sched(state: number, due: number): FSRSState {
  return { kind: 'fsrs', stability: 0, difficulty: 0, reps: state === 0 ? 0 : 1, lapses: 0, state, step: 0, lastReview: null, due }
}
function card(id: string, createdAt: number, s: FSRSState): SessionCard {
  return { card: { id, deckId: 'd', front: id, back: id, createdAt, updatedAt: createdAt, scheduling: s } as Card, settings: S }
}

describe('buildSessionCards', () => {
  it('sequential: reviews by due, then new by createdAt', () => {
    const cards = [
      card('n2', 20, sched(0, 0)),
      card('r1', 5, sched(2, 100)),
      card('n1', 10, sched(0, 0)),
    ]
    const out = buildSessionCards(cards, 'sequential').map((c) => c.card.id)
    expect(out).toEqual(['r1', 'n1', 'n2'])
  })
  it('random: preserves the full set of new cards', () => {
    const cards = [card('n1', 10, sched(0, 0)), card('n2', 20, sched(0, 0)), card('n3', 30, sched(0, 0))]
    const out = buildSessionCards(cards, 'random').map((c) => c.card.id).sort()
    expect(out).toEqual(['n1', 'n2', 'n3'])
  })
})

describe('ReviewSession', () => {
  it('serves due cards in order and counts reviewed/remaining', () => {
    const now = 1000
    const s = new ReviewSession([card('a', 1, sched(2, 0)), card('b', 2, sched(2, 0))])
    expect(s.remaining).toBe(2)
    expect(s.next(now)!.card.id).toBe('a')
    s.grade(now, sched(2, now + 5 * 86_400_000)) // graduated far out → leaves session
    expect(s.reviewed).toBe(1)
    expect(s.next(now)!.card.id).toBe('b')
  })

  it('re-inserts a short-step card and serves it once due', () => {
    const now = 1000
    const s = new ReviewSession([card('a', 1, sched(0, 0)), card('b', 2, sched(2, 0))])
    expect(s.next(now)!.card.id).toBe('a')
    s.grade(now, sched(1, now + 600_000)) // 'a' → learning, due in 10m (within window)
    // 'b' is due now, so it comes next; 'a' is not yet due
    expect(s.next(now)!.card.id).toBe('b')
    s.grade(now, sched(2, now + 5 * 86_400_000))
    // only 'a' left, still 10m out but within learn-ahead → served early
    expect(s.next(now)!.card.id).toBe('a')
  })

  it('learn-ahead: does not serve a step card beyond the window', () => {
    const now = 1000
    const s = new ReviewSession([card('a', 1, sched(1, now + LEARN_AHEAD_MS + 60_000))])
    expect(s.next(now)).toBeNull()
  })

  it('empty session → null', () => {
    expect(new ReviewSession([]).next(1000)).toBeNull()
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run --project unit src/features/review/session.test.ts`
Expected: FAIL — `Failed to resolve import "./session"`.

- [ ] **Step 3: Implement the session model**

Create `src/features/review/session.ts`:

```ts
import type { Card, DeckSettings, FSRSState, InsertionOrder } from '../../domain/models'
import { shuffle } from './dueOverview'

/** How early a not-yet-due learning card may be shown so the user never waits. */
export const LEARN_AHEAD_MS = 20 * 60_000

export interface SessionCard {
  card: Card
  settings: DeckSettings
}

/** Initial single-deck order: due reviews first (by due), then new cards in
 *  the deck's insertion order. */
export function buildSessionCards(cards: SessionCard[], order: InsertionOrder): SessionCard[] {
  const news = cards.filter((c) => c.card.scheduling.state === 0)
  const rest = cards.filter((c) => c.card.scheduling.state !== 0)
  const orderedNew =
    order === 'random'
      ? shuffle(news)
      : news.slice().sort((a, b) => a.card.createdAt - b.card.createdAt)
  const orderedRest = rest.slice().sort((a, b) => a.card.scheduling.due - b.card.scheduling.due)
  return [...orderedRest, ...orderedNew]
}

/** One review sitting. Holds an ordered working queue; step cards re-insert and
 *  are shown when due, or a little early via learn-ahead. Pure — no storage. */
export class ReviewSession {
  private queue: SessionCard[]
  private current: SessionCard | null = null
  private _reviewed = 0

  constructor(cards: SessionCard[]) {
    this.queue = cards.slice()
  }

  get remaining(): number {
    return this.queue.length + (this.current ? 1 : 0)
  }

  get reviewed(): number {
    return this._reviewed
  }

  next(now: number): SessionCard | null {
    // First card already due, in queue order.
    let pick = this.queue.findIndex((c) => c.card.scheduling.due <= now)
    if (pick < 0) {
      // Nothing due — learn-ahead: the earliest-due card, if within the window.
      if (this.queue.length === 0) {
        this.current = null
        return null
      }
      let earliest = 0
      for (let k = 1; k < this.queue.length; k++) {
        if (this.queue[k].card.scheduling.due < this.queue[earliest].card.scheduling.due) earliest = k
      }
      if (this.queue[earliest].card.scheduling.due - now > LEARN_AHEAD_MS) {
        this.current = null
        return null
      }
      pick = earliest
    }
    const [chosen] = this.queue.splice(pick, 1)
    this.current = chosen
    return chosen
  }

  grade(now: number, next: FSRSState): void {
    const cur = this.current
    if (!cur) return
    this._reviewed += 1
    this.current = null
    const stillStepping = (next.state === 1 || next.state === 3) && next.due - now <= LEARN_AHEAD_MS
    if (stillStepping) {
      this.queue.push({ ...cur, card: { ...cur.card, scheduling: next } })
    }
  }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npx vitest run --project unit src/features/review/session.test.ts`
Expected: PASS.

- [ ] **Step 5: Typecheck**

Run: `npm run typecheck`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/features/review/session.ts src/features/review/session.test.ts
git commit -m "feat(review): dynamic session queue with learn-ahead re-insertion

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 7: Wire `ReviewPage` to the step machine + session; sub-day interval labels; `isNew`

**Files:**
- Modify: `src/features/review/dueOverview.ts` (`isNew` → `state === 0`)
- Modify: `src/features/review/dueOverview.test.ts` (helper sets `state`)
- Modify: `src/features/review/GradeButtons.tsx` (sub-day interval labels)
- Rewrite: `src/features/review/ReviewPage.tsx` (session + `nextStates`)

**Interfaces:**
- Consumes: `nextStates` (Task 5), `ReviewSession`/`buildSessionCards`/`SessionCard` (Task 6), `settingsToParams` no longer needed in `ReviewPage`.

- [ ] **Step 1: Update `isNew` and its test (RED then GREEN)**

In `src/features/review/dueOverview.test.ts`, change the `fsrs` helper (line 7-9) so new cards carry `state: 0`:

```ts
function fsrs(reps: number): SchedulingState {
  return { kind: 'fsrs', stability: 1, difficulty: 5, reps, lapses: 0, state: reps === 0 ? 0 : 2, step: 0, lastReview: null, due: 0 }
}
```

Run: `npx vitest run --project unit src/features/review/dueOverview.test.ts`
Expected: FAIL — `isNew(fsrs(0))` is still true only because `reps === 0`; the `loadDueOverview` new-count assertions now depend on the not-yet-updated `isNew`. (If it happens to pass, proceed — the source change below is still required for correctness.)

In `src/features/review/dueOverview.ts`, change `isNew`:

```ts
/** A card the user has never studied (still in the New state). */
export function isNew(s: SchedulingState): boolean {
  return s.state === 0
}
```

Run: `npx vitest run --project unit src/features/review/dueOverview.test.ts`
Expected: PASS.

- [ ] **Step 2: Sub-day interval labels on the grade buttons**

In `src/features/review/GradeButtons.tsx`, replace `formatInterval` and its call so learning steps read as minutes/hours instead of `1d`:

```ts
/** Human-readable interval from a millisecond delta: 1m / 10m / 2h / 6d / 2mo / 1y. */
function formatInterval(ms: number): string {
  const min = Math.round(ms / 60_000)
  if (min < 60) return `${Math.max(1, min)}m`
  const hrs = Math.round(ms / 3_600_000)
  if (hrs < 24) return `${hrs}h`
  const days = Math.round(ms / 86_400_000)
  if (days < 30) return `${days}d`
  if (days < 365) return `${Math.round(days / 30)}mo`
  return `${Math.round(days / 365)}y`
}
```

and the label (drop the now-unused `MS_PER_DAY` import):

```tsx
          <span className="grade-hint">{formatInterval(nexts[grade].due - now)}</span>
```

- [ ] **Step 3: Rewrite `ReviewPage` to drive the session**

Replace `src/features/review/ReviewPage.tsx` with:

```tsx
import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { useLiveQuery } from 'dexie-react-hooks'
import type { DeckSettings, FSRSState, Grade } from '../../domain/models'
import { nextStates } from '../../domain/scheduler/reviewScheduler'
import { useStorage } from '../../data/StorageContext'
import { PageHeader } from '../../ui/PageHeader'
import { MarkdownView } from '../cards/MarkdownView'
import { GradeButtons } from './GradeButtons'
import { loadDueOverview, shuffle } from './dueOverview'
import { ReviewSession, buildSessionCards, type SessionCard } from './session'

export function ReviewPage() {
  const { deckId } = useParams()
  const storage = useStorage()

  const sessionRef = useRef<ReviewSession | null>(null)
  const [current, setCurrent] = useState<SessionCard | null>(null)
  const [ready, setReady] = useState(false)
  const [revealed, setRevealed] = useState(false)
  const [nexts, setNexts] = useState<Record<Grade, FSRSState> | null>(null)
  const [revealedAt, setRevealedAt] = useState(0)
  const [schedError, setSchedError] = useState(false)

  const deck = useLiveQuery(() => (deckId ? storage.getDeck(deckId) : undefined), [deckId])
  const deckName = deckId ? (deck?.name ?? '') : 'All decks'
  const backTo = deckId ? `/decks/${deckId}` : '/'

  useEffect(() => {
    let active = true
    const now = Date.now()
    async function build(): Promise<SessionCard[]> {
      if (deckId) {
        const d = await storage.getDeck(deckId)
        if (!d) return []
        const due = await storage.dueCards(deckId, now)
        const cards = due.map((card) => ({ card, settings: d.settings }))
        return buildSessionCards(cards, d.settings.insertionOrder)
      }
      const ov = await loadDueOverview(storage, now)
      const settingsById = new Map<string, DeckSettings>(ov.decks.map((o) => [o.deck.id, o.deck.settings]))
      return shuffle(ov.queue).map((card) => ({ card, settings: settingsById.get(card.deckId)! }))
    }
    void build().then((cards) => {
      if (!active) return
      const session = new ReviewSession(cards)
      sessionRef.current = session
      setCurrent(session.next(Date.now()))
      setReady(true)
    })
    return () => {
      active = false
    }
  }, [deckId, storage])

  const fetchNexts = useCallback((sc: SessionCard, now: number) => {
    setSchedError(false)
    setNexts(null)
    void nextStates(sc.card.scheduling, sc.settings, now)
      .then(setNexts)
      .catch((err: unknown) => {
        console.error('nextStates failed', err)
        setSchedError(true)
      })
  }, [])

  const reveal = useCallback(() => {
    if (!current || revealed) return
    const now = Date.now()
    setRevealed(true)
    setRevealedAt(now)
    fetchNexts(current, now)
  }, [current, revealed, fetchNexts])

  const grade = useCallback(
    async (g: Grade) => {
      const session = sessionRef.current
      if (!current || !nexts || !session) return
      await storage.updateCard(current.card.id, { scheduling: nexts[g] })
      session.grade(Date.now(), nexts[g])
      setCurrent(session.next(Date.now()))
      setRevealed(false)
      setNexts(null)
      setSchedError(false)
    },
    [current, nexts, storage],
  )

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!current) return
      if (!revealed) {
        if (e.code === 'Space' || e.key === 'Enter') {
          e.preventDefault()
          reveal()
        }
        return
      }
      const byKey: Record<string, Grade> = { '1': 'again', '2': 'hard', '3': 'good', '4': 'easy' }
      const g = byKey[e.key]
      if (g) {
        e.preventDefault()
        void grade(g)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [current, revealed, reveal, grade])

  if (!ready) return null

  if (current === null) {
    const reviewed = sessionRef.current?.reviewed ?? 0
    if (reviewed === 0) {
      return (
        <div className="page-body">
          <div className="empty-state">
            <div className="ico">🌙</div>
            <h3>Nothing due</h3>
            <p>{deckId ? 'Nothing due in this deck right now.' : 'Nothing due across your decks right now.'}</p>
            <Link to={backTo} className="btn btn-ghost cta">
              {deckId ? 'Back to deck' : 'Back to Today'}
            </Link>
          </div>
        </div>
      )
    }
    return (
      <div className="page-body">
        <div className="empty-state">
          <div className="ico">🎉</div>
          <h3>Review complete</h3>
          <p>
            {reviewed} card{reviewed === 1 ? '' : 's'} done. Nice work.
          </p>
          <Link to={backTo} className="btn btn-primary cta">
            {deckId ? 'Back to deck' : 'Back to Today'}
          </Link>
        </div>
      </div>
    )
  }

  const reviewed = sessionRef.current?.reviewed ?? 0
  const total = reviewed + (sessionRef.current?.remaining ?? 0)
  const title = (
    <>
      <span className="review-pos">
        {reviewed + 1} / {total}
      </span>
      <span className="review-deck">{deckName}</span>
    </>
  )

  return (
    <>
      <PageHeader
        title={title}
        actions={
          <Link to={backTo} className="btn btn-ghost">
            End session
          </Link>
        }
      />
      <div className="review">
        {!revealed ? (
          <div className="review-stage">
            <div className="review-card">
              <div className="review-q">
                <MarkdownView source={current.card.front} />
              </div>
            </div>
            <button className="review-show" onClick={reveal}>
              Show answer <span className="kbd">space</span>
            </button>
          </div>
        ) : (
          <div className="review-stage reveal-enter">
            <div className="review-card revealed">
              <div className="review-q">
                <MarkdownView source={current.card.front} />
              </div>
              <hr className="review-rule" />
              <p className="answer-label">Answer</p>
              <div className="review-a">
                <MarkdownView source={current.card.back} />
              </div>
            </div>
            {nexts && <GradeButtons nexts={nexts} now={revealedAt} onGrade={grade} />}
            {schedError && !nexts && (
              <div className="empty-state">
                <p>Couldn&#39;t schedule this card.</p>
                <button className="btn btn-ghost" onClick={() => fetchNexts(current, revealedAt)}>
                  Retry
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </>
  )
}
```

- [ ] **Step 4: Typecheck**

Run: `npm run typecheck`
Expected: PASS. (`GradeButtons` still takes `nexts: Record<Grade, SchedulingState>`; `FSRSState` is assignable.)

- [ ] **Step 5: Run the review + full suite; regen screenshots if needed**

Run: `npm test`
Expected: PASS. The `reveal.browser.test.tsx` flows still hold (a new card reveals grade buttons; a rejected `previewNextStates` — which `nextStates` awaits — surfaces the recoverable error + Retry).

If a Playwright screenshot assertion fails **only** because a grade-button label changed from `1d` to a sub-day value (`1m`/`10m`), refresh the baselines: `npx vitest run --project browser --update` — then eyeball the diff before committing.

- [ ] **Step 6: Commit**

```bash
git add src/features/review/dueOverview.ts src/features/review/dueOverview.test.ts src/features/review/GradeButtons.tsx src/features/review/ReviewPage.tsx
git commit -m "feat(review): drive review from the step machine + dynamic session

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

If screenshots were regenerated in Step 5, `git add` the updated `__screenshots__` PNGs in the same commit.

---

## Final verification (after Task 7)

- [ ] `npm run typecheck` — clean.
- [ ] `npm test` — all unit + browser tests green.
- [ ] `cd src-tauri && cargo test` — still green (no Rust change).
- [ ] `grep -rn "graduatingInterval\|easyInterval\|DEFAULT_DECK_FSRS_PARAMS" src` — prints nothing.
- [ ] Manual smoke (`npm run app:dev`): new card `Again` re-shows within the session and graduates after its steps; review card `Again` enters relearning; a deck with a low `desiredRetention` shows visibly shorter intervals. (Automated tests use the fake scheduler, so this native path is the only end-to-end check.)

## Self-review notes (coverage against the spec)

- Data model (`step`, v7, dropped fields, no Rust change) → Tasks 2, 3.
- Step tokenizing (`parseSteps`/`parseStepsMs`) → Task 1.
- Step machine (all state/grade transitions, field-carry, clamps) → Task 5.
- Seam change (`params`, drop default) → Task 4.
- Session queue (build order, learn-ahead, re-insertion) → Task 6; `ReviewPage` driver + `isNew` → Task 7.
- Sub-day interval legibility (supports "steps are meaningful") → Task 7, Step 2 — a small extension beyond the spec's "GradeButtons unchanged" note; flagged for the reviewer.
</content>
