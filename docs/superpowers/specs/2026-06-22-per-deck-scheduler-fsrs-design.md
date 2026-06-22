# Per-deck scheduler (SM-2 + FSRS) — design

_Date: 2026-06-22_

## Goal

Add **FSRS** as a second spaced-repetition algorithm (ROADMAP mid-term item 4),
chosen **per deck**, slotting behind the existing `Scheduler` interface. FSRS
comes from the [`ts-fsrs`](https://github.com/open-spaced-repetition/ts-fsrs)
library. Existing decks keep SM-2; new decks default to FSRS.

## Decisions (from brainstorming)

- **Library, not hand-rolled** — wrap `ts-fsrs` behind our `Scheduler` interface,
  adapting its `Date`/`Card` API to our pure `(state, grade, now)` numeric-ms
  contract. We only use the forward scheduler (default global weights), not the
  optimizer.
- **Per-deck choice, fixed at creation** — each deck stores which algorithm it
  uses; once created it can't change. This means each card's state always matches
  its deck's algorithm, so there is **zero cross-algorithm state conversion**.
- **No migration of existing card state** — existing decks stay SM-2, so their
  card state is already valid. We only stamp a `schedulerKind`/`kind` discriminant
  onto legacy records (see §3, §5).
- **New decks default to FSRS**; SM-2 remains selectable.

## 1. Data model (`src/domain/models.ts`)

`SchedulingState` becomes a **discriminated union** on a `kind` field, so every
card is self-describing — the correct scheduler and UI logic resolve from the
card alone, with no deck lookup:

```ts
export type SchedulerKind = 'sm2' | 'fsrs'

export interface SM2State {
  kind: 'sm2'
  repetitions: number
  intervalDays: number
  easeFactor: number
  due: number
}

export interface FSRSState {
  kind: 'fsrs'
  stability: number
  difficulty: number
  reps: number
  lapses: number
  state: number          // ts-fsrs State enum: 0 New / 1 Learning / 2 Review / 3 Relearning
  lastReview: number | null
  due: number
}

export type SchedulingState = SM2State | FSRSState
```

`Deck` gains `schedulerKind: SchedulerKind`. It is read in exactly **one** place —
`createCard`, to stamp the initial scheduling state. Thereafter
`card.scheduling.kind` drives all dispatch.

_Alternative considered:_ keep the deck as the sole source of truth and thread
`schedulerKind` into every consumer. Rejected — putting `kind` on the state keeps
`cardStatus` / `GradeButtons` / `ReviewPage` dispatch local and self-contained.

## 2. Scheduler module (`src/domain/scheduler/`)

- `sm2.ts` — unchanged logic, except `initial`/`next` now stamp `kind: 'sm2'`.
- `fsrs.ts` — new `FSRSScheduler implements Scheduler`, a thin adapter over
  `ts-fsrs`. A single module-level instance:
  `fsrs(generatorParameters({ enable_fuzz: false, enable_short_term: false, request_retention: 0.9 }))`
  — **fuzz off** for deterministic tests, **short-term off** to stay day-granular
  like the rest of the app.
  - `initial(now)` → `createEmptyCard(new Date(now))` mapped to an `FSRSState`
    (`kind: 'fsrs'`, `due = now`, `reps = 0`, `lapses = 0`, `state = 0`,
    `lastReview = null`, `stability`/`difficulty` from the empty card).
  - `next(state, grade, now)` → reconstruct a `ts-fsrs` `Card` from the
    `FSRSState`, call `.next(card, new Date(now), ratingOf(grade))`, map the
    returned card back to `FSRSState` (Date↔epoch-ms; `last_review` → `lastReview`).
  - `ratingOf`: `again→1`, `hard→2`, `good→3`, `easy→4` (ts-fsrs `Rating`).
- `index.ts` — replace the `scheduler` singleton with a registry:
  `getScheduler(kind: SchedulerKind): Scheduler`. `MS_PER_DAY` stays exported.

The `Scheduler` interface (`initial(now)`, `next(state, grade, now)`) is
unchanged; each implementation receives the union and operates on its own variant
(dispatch by kind guarantees the match).

## 3. Storage

- `Storage.createDeck(name, kind?: SchedulerKind)` — `kind` defaults to `'sm2'`
  so the existing test/seed call sites stay unchanged; the UI passes `'fsrs'`
  explicitly.
- `DexieStorage` drops its injected-`scheduler` constructor argument and uses
  `getScheduler` internally. `createCard` fetches the deck, reads its
  `schedulerKind`, and calls that scheduler's `initial`.
- **Dexie v2 migration** (`db.ts`): bump to `version(2)` with an `.upgrade()` that
  stamps `schedulerKind: 'sm2'` on existing decks and `kind: 'sm2'` on existing
  cards' `scheduling`. This keeps data at rest always satisfying the union — no
  scattered `?? 'sm2'` read-time defaults. (Schema/indexes are unchanged; the
  version bump exists only to run the upgrade.)

## 4. Consumers

- `cardStatus` (`DeckDetailPage.tsx`): the "new" check switches on kind —
  `repetitions === 0` for `sm2`, `reps === 0` for `fsrs`. The `due`-based
  due/scheduled labelling is unchanged.
- `GradeButtons.tsx` and `ReviewPage.tsx`: resolve `getScheduler(scheduling.kind)`
  instead of importing the singleton. `GradeButtons` computes each button's
  interval label from the result's `due`
  (`Math.max(1, Math.round((next.due - now) / MS_PER_DAY))`) rather than reading
  `.intervalDays` — identical output for SM-2, and works for FSRS too.

## 5. Backup (`src/data/backup.ts`)

- `DeckBackup` gains `schedulerKind: SchedulerKind`. `collectBackup` reads it from
  the deck.
- `parseBackup` normalizes **legacy** files: a deck without `schedulerKind` →
  `'sm2'`; a card `scheduling` without `kind` → stamped `'sm2'`. `isScheduling`
  validates both variants (discriminates on `kind`, defaulting to the SM-2 shape
  when absent). Old exports still import.
- `BackupFile.version` stays `1` — the format is backward-compatible (new
  optional fields, legacy normalized on read).

## 6. UI

- Deck-create form (`DeckListPage.tsx`): a small algorithm picker
  ("FSRS — recommended" / "SM-2 — classic"), defaulting to FSRS; its value is
  passed to `createDeck`.
- Deck detail header (`DeckDetailPage.tsx`): a small badge showing the deck's
  algorithm.

## 7. Dependency

Add `ts-fsrs` to `dependencies`.

## 8. Testing

- `fsrs.test.ts` — `initial` shape (`kind: 'fsrs'`, due now, reps 0); `next`
  produces a future `due` that grows across `good`→`easy` and a near-term `due`
  on `again` (lapse increments `lapses`); `kind` stamping; determinism (fixed
  `now`, fuzz off → stable output).
- Scheduler registry test — `getScheduler` returns the right implementation.
- `cardStatus` — add the FSRS "new" case.
- `backup` — round-trip an FSRS deck/card; legacy normalization (deck without
  `schedulerKind`, card scheduling without `kind` → both load as `sm2`).
- Dexie v2 migration test — open a v1 DB with un-stamped deck + card, reopen as
  `RemDB`, assert `schedulerKind`/`kind` stamped to `'sm2'`.
- `createCard` uses the deck's scheduler — an FSRS deck's new card gets
  `kind: 'fsrs'` state.
- Review-cycle integration — add an FSRS variant alongside the existing SM-2 one.
- UI — creating a deck with FSRS selected produces FSRS-scheduled cards.

## Out of scope

- FSRS **optimizer** / personalized weights (needs a review-history log we don't
  keep). Default global weights only.
- Sub-day learning steps (short-term scheduling) — disabled to stay day-granular.
- Changing a deck's algorithm after creation.
