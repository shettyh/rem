# Rust FSRS scheduling core — design

_Date: 2026-06-28_

## Context

This is **sub-project #2** of a larger effort to (a) implement the full Anki-grade
per-deck "Deck options" screen from `rem.dc.html`, and (b) port scheduling from
`ts-fsrs` (TypeScript) to `fsrs-rs` (Rust). The agreed decomposition:

1. Deck options screen + `DeckSettings` data model
2. **Rust FSRS scheduling core ← this spec**
3. Anki-grade review queue (learning/relearning steps, daily caps, insertion
   order, leech, burying)
4. Custom study
5. FSRS weight optimization (train personalised weights from review logs)

We build **#2 first** to lay the engine foundation. #1 and #3 thread real per-deck
parameters through the API this sub-project establishes.

## Goal

Move the FSRS algorithm from [`ts-fsrs`](https://github.com/open-spaced-repetition/ts-fsrs)
into [`fsrs-rs`](https://github.com/open-spaced-repetition/fsrs-rs), exposed via a
Tauri command, behind the existing `Scheduler` seam. The Rust side **owns the
review transition** and returns all four grade outcomes (`again/hard/good/easy`) as
complete next-states in **one call**.

Success = reviewing a card reschedules it via Rust, the four grade buttons show
the predicted intervals from that single cached result, the algorithm is
`cargo test`-covered, and `ts-fsrs` is removed from the runtime.

## Decisions (from brainstorming)

- **Full port to Rust** (not hybrid). The FSRS memory model + transition run in
  `fsrs-rs`; TS calls it over Tauri IPC.
- **One `previewNextStates()` call, cache the 4.** `fsrs-rs`'s `next_states()`
  returns all four outcomes at once. `ReviewPage` awaits it once on "show answer",
  caches the four; grading picks one **locally** (sync); `GradeButtons` renders the
  cached previews instead of computing them. Collapses today's "4 calls in
  GradeButtons + 1 in ReviewPage" into a single IPC round-trip per card.
- **Rust owns the full `FSRSState` transition** — reps, lapses, state, stability,
  difficulty, `last_review`, `due`. This sets up the #3 queue (steps/leech/burying)
  to live in Rust too.
- **`initial()` stays synchronous TS** — the one deliberate exception. Creating an
  empty new card needs no FSRS math, so keeping it pure avoids an IPC round-trip on
  every card creation and keeps card-creation unit-testable.
- **FSRS-equivalent, not bit-identical.** `fsrs-rs` is FSRS-6 (21 default params)
  vs `ts-fsrs`'s defaults; intervals will differ slightly. Existing cards keep
  their stored `due`; only **future** scheduling shifts. No data migration.
- **Tests use a deterministic `FakeScheduler`, not `ts-fsrs`.** The FSRS algorithm
  is verified in `cargo test`; TS tests verify **wiring**, on a non-FSRS fake
  selected by `isTauri()` (mirrors how sync is gated today). `ts-fsrs` is dropped.

## 1. Rust side (`src-tauri`)

### Dependency
Add `fsrs` (latest; FSRS-6, `DEFAULT_PARAMETERS: [f32; 21]`) to `Cargo.toml`. No
direct `chrono` use — elapsed days are computed from timestamps passed in.

### New module `src-tauri/src/fsrs_sched.rs`

Serde DTOs mirror the domain model. Timestamps are epoch **ms** as `i64`;
stability/difficulty are `f32`. Field names use serde `rename_all = "camelCase"`
to match the TS payload (`lastReview`, etc.).

```rust
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsrsStateDto {
    pub stability: f32,
    pub difficulty: f32,
    pub reps: u32,
    pub lapses: u32,
    pub state: u8,                 // 0 New / 1 Learning / 2 Review / 3 Relearning
    pub last_review: Option<i64>,  // epoch ms
    pub due: i64,                  // epoch ms
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckFsrsParams {
    pub desired_retention: f32,
    pub maximum_interval: u32,     // days
    pub weights: Option<Vec<f32>>, // None → DEFAULT_PARAMETERS
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStatesDto {
    pub again: FsrsStateDto,
    pub hard: FsrsStateDto,
    pub good: FsrsStateDto,
    pub easy: FsrsStateDto,
}
```

### Command

```rust
#[tauri::command]
pub fn fsrs_next_states(
    state: FsrsStateDto,
    now: i64,                 // epoch ms
    params: DeckFsrsParams,
) -> Result<NextStatesDto, String>
```

Logic:

1. `let fsrs = FSRS::new(params.weights.as_deref())?` (passing `None` → defaults).
2. `days_elapsed = state.last_review.map(|t| max(0, (now - t) / 86_400_000)).unwrap_or(0) as u32`.
3. `current = if state.reps == 0 { None } else { Some(MemoryState { stability, difficulty }) }`.
4. `let ns = fsrs.next_states(current, params.desired_retention, days_elapsed)?`.
5. For each grade `g` in `[again, hard, good, easy]`, build the next state from
   `ns.<g>` (`{ memory: MemoryState, interval: f32 }`):
   - `interval_days = clamp(round(interval) as u32, 1, params.maximum_interval)`
   - `due = now + interval_days as i64 * 86_400_000`
   - `reps = state.reps + 1`
   - `lapses = state.lapses + (g == again && state.state == 2 ? 1 : 0)`
   - `state = 2` (Review) — #2 has no learning/relearning steps; #3 introduces them
   - `last_review = Some(now)`
   - `stability = ns.<g>.memory.stability`, `difficulty = ns.<g>.memory.difficulty`
6. Return `NextStatesDto`.

`MS_PER_DAY = 86_400_000` as a module const.

### Registration & tests

- Add `mod fsrs_sched;` and `fsrs_sched::fsrs_next_states` to the `invoke_handler!`
  list in `lib.rs`.
- `cargo test` in `fsrs_sched.rs`:
  - **new card** (`reps == 0`, `last_review: None`) → four states, each `reps == 1`,
    `state == 2`, `due > now`, all four intervals `>= 1`.
  - **interval ordering** `again <= hard <= good <= easy` for a typical review.
  - **lapse**: `again` from a Review-state card increments `lapses`; other grades
    do not.
  - **max-interval clamp**: tiny `maximum_interval` (e.g. 5) caps every `due` at
    `now + 5d`.
  - **due math**: `due == now + interval_days * MS_PER_DAY`.

## 2. TS side

### `Scheduler` interface (`src/domain/scheduler/Scheduler.ts`)

```ts
export interface Scheduler {
  /** Scheduling state for a brand-new card (immediately due). Pure, synchronous. */
  initial(now: number): SchedulingState
  /** All four grade outcomes for the next review, in one shot. */
  previewNextStates(state: SchedulingState, now: number): Promise<Record<Grade, SchedulingState>>
}
```

`next()` is removed.

### Implementations

- **`TauriFsrsScheduler`** (`src/domain/scheduler/tauriFsrs.ts`): a thin bridge,
  same shape as `TauriGitBridge`.
  - `previewNextStates(state, now)` → `invoke<NextStatesDto>('fsrs_next_states', { state, now, params })`,
    then map each DTO branch into our `FSRSState` (1:1; both already carry the same
    fields).
  - `params` for #2 is a module constant `DEFAULT_DECK_FSRS_PARAMS = { desiredRetention: 0.9, maximumInterval: 36500, weights: null }`. **#1/#3 replace this constant with the deck's stored settings** — the `DeckFsrsParams` argument already exists, so no signature change later.
  - `initial(now)` builds the empty new-card `FSRSState` in TS (`state: 0`,
    `stability: 0`, `difficulty: 0`, `reps: 0`, `lapses: 0`, `lastReview: null`,
    `due: now`).
- **`FakeScheduler`** (`src/domain/scheduler/fakeScheduler.ts`): deterministic,
  non-FSRS, for tests + non-Tauri dev. `initial()` same empty card;
  `previewNextStates()` returns four states with fixed offsets
  (`again +0d`, `hard +1d`, `good +3d`, `easy +7d`), `reps + 1`, and `lapses + 1`
  on `again`. Enough to assert wiring; FSRS accuracy lives in `cargo test`.

### Selection (`src/domain/scheduler/index.ts`)

```ts
import { isTauri } from '@tauri-apps/api/core'
export function getScheduler(): Scheduler {
  return isTauri() ? tauriFsrs : fakeScheduler  // singletons
}
```

`SchedulerKind` is still `'fsrs'`; `getScheduler()` no longer needs the kind
argument (single algorithm). Callers in `db.ts`, `DexieStorage.ts`, `backup.ts`
use `getScheduler().initial(now)` (synchronous, unchanged behaviour).

### Review path

- **`ReviewPage.tsx`**: on "show answer", `const nexts = await getScheduler().previewNextStates(current.scheduling, Date.now())`, store `nexts` in component state. On grade `g`: `await storage.updateCard(current.id, { scheduling: nexts[g] })`. Reset `nexts` when advancing to the next card.
- **`GradeButtons.tsx`**: takes `nexts: Record<Grade, SchedulingState>` as a prop
  (replacing the current `scheduling` + internal `getScheduler().next()` calls).
  Each button's interval label = `Math.max(1, Math.round((nexts[grade].due - now) / MS_PER_DAY))`.

### Removals

- Delete `src/domain/scheduler/fsrs.ts` and `fsrs.test.ts` (algorithm now in Rust).
- Remove `ts-fsrs` from `package.json` dependencies.
- `MS_PER_DAY` stays exported from `index.ts` for the UI.

## 3. Out of scope (later sub-projects)

- Per-deck settings UI and storage of `DeckFsrsParams` (#1).
- Learning/relearning steps, daily caps, insertion order, leech, burying — the
  card stays in `state: 2` here and `previewNextStates` always returns day-grained
  Review intervals (#3).
- Weight optimization / review-log capture (#5).

## 4. Verification

1. `cd src-tauri && cargo test` — green for all transition cases in §1.
2. `npm run typecheck` — green.
3. `npm test` (Vitest + Playwright) — green using `FakeScheduler`; review-flow
   browser test exercises reveal → four interval labels → grade → reschedule.
4. Manual (`npm run app:dev`): review a card → it reschedules; the four buttons
   show sensible, ascending intervals.
5. `grep ts-fsrs package.json` returns nothing.

## 5. Accepted trade-offs

- FSRS math is no longer unit-tested in TS (moved to `cargo test`); browser tests
  run on the fake.
- Review-time scheduling becomes **async + native-only** — the deliberate
  consequence of the full Rust port.
- One algorithm only; `getScheduler()` drops its `kind` argument. If a second
  algorithm is ever wanted, the seam is still there to re-parameterise.
