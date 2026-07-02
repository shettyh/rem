# Anki-grade review queue — learning/relearning steps + real deck params + insertion order — design

_Date: 2026-07-02_

## Context

This is the first slice of **sub-project #3** (the Anki-grade review queue) from the
per-deck-settings + Rust-FSRS effort. Sub-projects #1 (Deck options screen +
`DeckSettings`) and #2 (Rust FSRS scheduling core) are shipped on `main`.

#3 as originally scoped bundles five mechanisms: learning/relearning steps, daily
caps, insertion order, leech, and burying. Several of those depend on
infrastructure that does not exist yet (a review log / daily counters for caps; a
`suspended` flag and a note/sibling concept for leech and burying). We therefore
**slice #3**:

- **#3a — this spec:** learning/relearning steps + thread each deck's real FSRS
  params + insertion order. Self-contained in the scheduler seam and a new session
  queue; no new tables beyond one card field.
- **#3b (later):** daily caps (`newPerDay`/`maxReviews`) — needs the review-log /
  daily-counter infrastructure (also used by #5).
- **#3c (later):** leech detection + action, and burying — need a card `suspended`
  flag and a decision about "related" cards without a note model.

## Goal

Make grading behave like Anki: a new card walks its **learning steps**
(`1m 10m`) before graduating into FSRS review scheduling; a lapsed review card
walks its **relearning steps** (`10m`) before returning to review; and each deck's
own `desiredRetention` / `maximumInterval` finally drive the FSRS math instead of a
hardcoded default. Within a sitting, a card sent into a short step **re-appears in
the same session** (Option 1, dynamic queue).

Success =
- Grading a new card `Again` re-shows it in ~1 minute (first learning step), not a
  day later; `Good` walks it through the steps and graduates it via FSRS.
- Grading a review card `Again` records an FSRS lapse and drops the card into
  relearning steps before its next long-term interval.
- A deck with a non-default retention/max-interval schedules differently from a
  default deck (params are threaded, not hardcoded).
- New cards enter a single-deck session in the deck's `insertionOrder`.
- `graduatingInterval` / `easyInterval` are gone from the settings model and UI.
- Pure step-machine and session-queue logic is unit-tested; FSRS math stays
  `cargo`-tested.

## Decisions (from brainstorming)

- **Slice #3 — this cycle is steps + real params + insertion order only.** Daily
  caps, leech, and burying are explicitly deferred (see Context).
- **Approach A — classic fixed steps in TS; FSRS owns long-term.** Learning and
  relearning steps are fixed intervals cycled in TypeScript; FSRS (the existing
  Rust command) is consulted only for graduation and review-state transitions.
  Rejected Approach B (FSRS-native sub-day scheduling) because it contradicts the
  Anki-style settings UI shipped in #1 and makes every step an async round-trip.
- **FSRS picks every long-term interval; deck min/max only clamp it.** On
  graduation the card's `due` is FSRS's own good/easy interval (matching real Anki,
  where the graduating/easy-interval fields are disabled under FSRS). Relearning
  graduation clamps the FSRS good interval to `[minimumInterval, maximumInterval]`.
  Consequence: `graduatingInterval` and `easyInterval` are **dropped**.
- **`Hard` on a step repeats the current step.** A deliberate simplification of
  Anki (which averages current+next step).
- **Option 1 — dynamic session queue.** A card sent into a short step re-inserts
  into the current session; learn-ahead shows it a little early rather than making
  the user wait; cards still mid-step when the session ends persist their `due` and
  resume next session.
- **`reps` counts FSRS reviews only.** Learning-step grades do not increment `reps`
  and do not feed FSRS memory; graduation seeds FSRS as the card's first review.
  This keeps the graduation FSRS call on the clean `reps == 0 → current = None`
  path. `isNew` therefore shifts from `reps === 0` to `state === 0`.

## Data model

### `FSRSState` gains `step`

```ts
export interface FSRSState {
  kind: 'fsrs'
  stability: number
  difficulty: number
  reps: number
  lapses: number
  state: number          // 0 New / 1 Learning / 2 Review / 3 Relearning
  step: number           // index into the deck's learn/relearn steps; 0 when state ∈ {0,2}
  lastReview: number | null
  due: number
}
```

`step` is meaningful only while `state ∈ {1 Learning, 3 Relearning}`. It is written
whole with `scheduling`, so backup / snapshot / sync carry it for free.

### Migration (Dexie **v7**)

Backfill `step: 0` on every existing card's `scheduling` (mirrors the v6 backfill).
`initial()` and `FakeScheduler.emptyCard` set `step: 0`.

### `DeckSettings` — drop two fields

Remove `graduatingInterval` and `easyInterval` from `DeckSettings` and
`DEFAULT_DECK_SETTINGS`. No migration is needed: settings are stored whole, and
stray keys in existing decks / old backups are simply ignored by TS. Remove the two
corresponding `Stepper` controls (and any assertions) from `DeckSettingsPage`.

### Rust: no change

The step machine is TS-only. The existing `fsrs_next_states` command already
accepts `DeckFsrsParams`; graduation/review call it unchanged. The `state` payload
sent over IPC now carries an extra `step` field, which serde ignores
(`FsrsStateDto` has no `deny_unknown_fields`). Confirm with a wire round-trip in
review, but no Rust edit is expected.

## Step tokenizing (`src/domain/scheduler/steps.ts`)

Extract the existing `parseSteps` (currently in `DeckSettingsPage.tsx`, splits a
space-separated string into chip tokens) into a shared module and add duration
parsing:

```ts
/** Split a steps string into tokens, e.g. "1m 10m 1d" → ["1m","10m","1d"]. */
export function parseSteps(raw: string): string[]

/** Token → milliseconds. Units: s, m, h, d; a bare integer means minutes
 *  (Anki convention). Unparseable tokens are dropped. */
export function parseStepsMs(raw: string): number[]
```

`DeckSettingsPage` imports `parseSteps` from the new module (its chip rendering is
unchanged); `deckSettings.test.ts` updates its import path and gains `parseStepsMs`
cases. An empty step list is legal — a new card with no learning steps graduates on
the first non-Again grade; a review card with no relearning steps skips relearning
and takes the clamped FSRS lapse interval directly.

## Step machine (`src/domain/scheduler/reviewScheduler.ts`)

The heart of the slice. One entry point, replacing direct `getScheduler()` calls in
the review path:

```ts
export function settingsToParams(s: DeckSettings): DeckFsrsParams
// { desiredRetention: s.desiredRetention, maximumInterval: s.maximumInterval, weights: null }

export async function nextStates(
  scheduling: FSRSState,
  settings: DeckSettings,
  now: number,
): Promise<Record<Grade, FSRSState>>
```

`nextStates` makes **exactly one** FSRS seam call —
`getScheduler().previewNextStates(scheduling, settingsToParams(settings), now)` —
to obtain the four FSRS outcomes, then **overrides** the grades that are pure step
transitions. (The call is needed regardless, because `Easy` always graduates and
review/relearning-graduation grades are FSRS.) The FSRS outcomes are used as
follows per current `state`.

Let `L = parseStepsMs(settings.learnSteps)`, `R = parseStepsMs(settings.relearnSteps)`,
`i = scheduling.step`, and let `fsrs[g]` be the FSRS outcome for grade `g`.

### New (0) / Learning (1) — steps = `L`, `n = L.length`

| Grade | Result |
|---|---|
| again | `{ state:1, step:0, due: now + L[0] }` (memory unchanged: stability/difficulty stay 0, reps stays 0). If `n === 0`: `fsrs.again` (→ Review). |
| hard  | `{ state:1, step:i, due: now + L[i] }` (repeat current step). If `n === 0`: `fsrs.hard`. |
| good  | if `i+1 < n`: `{ state:1, step:i+1, due: now + L[i+1] }`; else **graduate** = `fsrs.good` (already `state:2`, `step:0`). If `n === 0`: `fsrs.good`. |
| easy  | **graduate** = `fsrs.easy` (`state:2`, `step:0`). |

Graduation uses `fsrs.good` / `fsrs.easy` verbatim: `reps` goes 0→1, memory seeded
by FSRS from `reps == 0 → current = None`, `due` = FSRS's interval (already clamped
to `maximumInterval` in Rust).

**Field-carry rule for step transitions.** A pure step transition (any row above
that is not a graduation/FSRS outcome) starts from `scheduling`, changes only
`state`, `step`, `due` as shown, sets `lastReview = now`, and leaves `reps`,
`stability`, `difficulty`, `lapses`, `kind` unchanged. FSRS-outcome rows use the
seam's returned state verbatim (then apply any documented `due` clamp).

### Review (2)

| Grade | Result |
|---|---|
| hard / good / easy | `fsrs[g]` verbatim (`state:2`, `step:0`, FSRS interval, params threaded). |
| again | **lapse → relearning.** Take memory from `fsrs.again` (stability dropped, `lapses` and `reps` incremented by Rust). If `R.length === 0`: keep `fsrs.again` but clamp `due` to `≥ now + minimumInterval·day` (`state:2`). Else override scheduling to `{ state:3, step:0, due: now + R[0] }`, keeping `fsrs.again`'s memory/lapses/reps. |

### Relearning (3) — steps = `R`, `n = R.length`

| Grade | Result |
|---|---|
| again | `{ state:3, step:0, due: now + R[0] }`, memory unchanged, **no** extra lapse. |
| hard  | `{ state:3, step:i, due: now + R[i] }` (repeat). |
| good  | if `i+1 < n`: `{ state:3, step:i+1, due: now + R[i+1] }`; else **graduate** = `fsrs.good` with `due` clamped to `[minimumInterval, maximumInterval]` days, `state:2`, `step:0`. |
| easy  | **graduate** = `fsrs.easy` with `due` clamped to `[minimumInterval, maximumInterval]` days, `state:2`, `step:0`. |

Relearning-graduation FSRS calls run on the card's current (lapse-updated) memory
(`reps > 0 → current = Some(memory)`), so the post-lapse interval reflects the
lower stability. The day-clamp recomputes `due = now + clamp(round((due-now)/day),
minimumInterval, maximumInterval)·day`.

## Scheduler seam change

`previewNextStates` gains a `params` argument (removing the
`DEFAULT_DECK_FSRS_PARAMS` hardcode that #2 left as a hook):

```ts
export interface Scheduler {
  initial(now: number): SchedulingState
  previewNextStates(
    state: SchedulingState,
    params: DeckFsrsParams,
    now: number,
  ): Promise<Record<Grade, SchedulingState>>
}
```

- `TauriFsrsScheduler` forwards `params` to the command (drops the module constant).
- `FakeScheduler` ignores `params` (stays deterministic) — its fixed offsets are
  enough to exercise the step machine and session wiring.
- `DEFAULT_DECK_FSRS_PARAMS` is deleted.

Only `reviewScheduler.nextStates` calls the seam; it builds `params` via
`settingsToParams`. Nothing else in the app calls `previewNextStates` directly.

## Dynamic session queue (`src/features/review/session.ts`)

A pure, storage-free model of one review sitting. `ReviewPage` becomes a thin
driver over it (loads due cards + per-card settings, drives `next`/`grade`,
persists each graded card).

```ts
interface SessionCard { card: Card; settings: DeckSettings }

class ReviewSession {
  constructor(cards: SessionCard[])       // pre-ordered by the builder
  next(now: number): SessionCard | null   // earliest-eligible, or null when done for now
  grade(now: number, next: FSRSState): void  // apply, re-insert if still a short step
  get remaining(): number
  get reviewed(): number
}
```

- **Build order.** Partition the due cards into new (`state === 0`) and the rest.
  New cards are ordered by `insertionOrder` (`sequential` → by `createdAt`;
  `random` → shuffled with the existing `shuffle`). The rest are ordered by `due`.
  Initial queue = **rest (by due) then new (by insertion order)**. `insertionOrder`
  is applied for **single-deck** review; the cross-deck ("All decks") path keeps
  its existing whole-queue shuffle.
- **`next(now)`** returns the earliest-due card. If that card is due
  (`due <= now`), show it. If nothing is due yet, show the earliest card only when
  it is within the **learn-ahead window** (`due - now <= LEARN_AHEAD_MS`,
  a `20 * 60_000` module const) — this shows a learning card slightly early instead
  of making the user watch a countdown. If the earliest card is beyond the window,
  the session is done for now (`null`); that card returns in a later session via its
  persisted `due`.
- **`grade(now, next)`** applies `next` to the current card and re-inserts it iff
  `next.state ∈ {1,3}` **and** `next.due - now <= LEARN_AHEAD_MS`; otherwise the
  card leaves the session (graduated to review, or a step longer than the window).
  The queue is kept sorted by `due`. Persistence to storage happens in `ReviewPage`
  regardless, so cross-session `due` is always durable.

### `ReviewPage`

- Load due cards (single deck: `storage.dueCards`; all decks: `loadDueOverview`),
  resolve each card's deck `settings` (single deck: the one deck; all decks: a
  `deckId → settings` map from `listDecks`), build a `ReviewSession`.
- On reveal: `const nexts = await nextStates(current.scheduling, current.settings, now)`;
  cache the four; `GradeButtons` renders them unchanged.
- On grade `g`: `await storage.updateCard(current.card.id, { scheduling: nexts[g] })`,
  then `session.grade(now, nexts[g])` and advance to `session.next(Date.now())`.
- Header counter follows the dynamic queue: show `reviewed` and `remaining` (exact
  format is a UI detail; the current `index+1 / total` becomes
  `reviewed+1 / (reviewed + remaining)`).
- The existing reveal/scheduling-error + Retry path is preserved (the async call is
  now `nextStates`, which still rejects if the Rust `invoke` fails).

### `isNew`

`dueOverview.isNew` changes from `s.reps === 0` to `s.state === 0` so learning
cards (which now keep `reps === 0`) are not miscounted as new. `newCount` /
`totalNew` semantics are unchanged for brand-new cards.

## Out of scope (later slices / sub-projects)

- Daily caps (`newPerDay`, `maxReviews`) and the review-log / daily-counter
  infrastructure — **#3b**.
- Leech detection + action (`leechThreshold`, `leechAction`) and burying
  (`buryRelated`) — **#3c** (needs a `suspended` flag / note model).
- FSRS weight optimization and review-log capture — **#5**.
- `showTimer` behaviour — untouched here.
- Learning-step grades feeding FSRS memory / short-term FSRS scheduling — not done
  (see trade-offs).

## Verification

1. `npm run typecheck` — green (seam signature + dropped fields propagate cleanly).
2. `npm test` (Vitest + Playwright) — green on the fake scheduler:
   - `steps.test.ts`: `parseStepsMs` units, bare-minutes, dropped tokens, empty.
   - `reviewScheduler.test.ts`: new-card learning progression (again resets to
     step 0; good walks steps; good on last step graduates; easy graduates); review
     `again` → relearning with lapse recorded; relearning progression + graduation
     clamped to `minimumInterval`; empty learn/relearn step lists; `settingsToParams`.
   - `session.test.ts`: build order (insertion `sequential`/`random`), re-insertion
     of a short-step card, learn-ahead window, session-done when nothing eligible.
   - `reviewCycle.test.ts`: updated for the new seam + step machine.
   - `ReviewPage` browser test + `reveal` browser test: reveal → four intervals →
     grade → reschedule; and a learning-step card re-appearing in the session.
   - `deckSettings.test.ts`: `parseSteps` import path + `parseStepsMs`; no
     graduating/easy references remain.
3. `cd src-tauri && cargo test` — unchanged, still green (no Rust edit).
4. Manual (`npm run app:dev`): a new card `Again` re-shows within the session and
   graduates after its steps; a review card `Again` enters relearning; a deck with
   a low `desiredRetention` shows visibly shorter intervals.
5. `grep -rn "graduatingInterval\|easyInterval\|DEFAULT_DECK_FSRS_PARAMS" src` —
   returns nothing.

## Accepted trade-offs

- **Learning-step grades do not feed FSRS.** FSRS memory is seeded once, at
  graduation, from the graduation grade. Faithful review-history capture (needed for
  #5 weight training) is deferred; the approximation is consistent with Approach A.
- **`Hard` repeats the current step** rather than averaging current+next (Anki).
- **`insertionOrder` applies to single-deck review only**; cross-deck review keeps
  its existing whole-queue shuffle.
- **Learn-ahead is a fixed 20-minute window**, not a deck setting (Anki exposes it;
  we do not, this slice).
- Review-time scheduling stays **async + native-only** (inherited from #2).
</content>
</invoke>
