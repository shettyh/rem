# Rust FSRS scheduling core — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move FSRS scheduling from `ts-fsrs` (TypeScript) into `fsrs-rs` (Rust), exposed via a Tauri command, behind the existing `Scheduler` seam, with the review transition computed in Rust and returned as all four grade outcomes in one call.

**Architecture:** A stateless Rust command `fsrs_next_states` takes a card's full FSRS state + `now` + deck params and returns the four complete next-states (`again/hard/good/easy`). The TS `Scheduler` interface keeps `initial()` synchronous (pure new-card creation) and replaces `next()` with async `previewNextStates()`. `getScheduler()` returns a real `TauriFsrsScheduler` in the app (`isTauri()`) and a deterministic `FakeScheduler` in tests/non-Tauri dev. `ReviewPage` calls `previewNextStates` once on reveal and caches the four; `GradeButtons` renders the cached previews.

**Tech Stack:** Rust + Tauri v2 + `fsrs` crate (FSRS-6, 21 default params); React + TypeScript; Vitest (jsdom + Playwright browser); `cargo test`.

## Global Constraints

- Timestamps are epoch **milliseconds**: `i64` in Rust, `number` in TS. `MS_PER_DAY = 86_400_000`.
- Stability/difficulty are `f32` in Rust, `number` in TS.
- All Rust DTOs use `#[serde(rename_all = "camelCase")]` so payload keys match TS (`lastReview`, `desiredRetention`, `maximumInterval`).
- Card `state` enum is `0 New / 1 Learning / 2 Review / 3 Relearning`. This sub-project only ever **emits `state = 2`** (no learning/relearning steps yet — that is sub-project #3).
- `initial()` stays **synchronous TS** (empty new card; no IPC).
- FSRS behavior is **equivalent, not bit-identical**, to the old `ts-fsrs`. No data migration; existing cards keep their stored `due`.
- Single algorithm: `getScheduler()` takes **no argument**. `SchedulerKind` (`'fsrs'`) stays in the model for `Deck`/backup/snapshot.
- `ts-fsrs` is removed from `package.json` at the end; no runtime TS FSRS math remains.
- Commits: identity `shettyh <manjunathshetty@live.com>`; conventional-commit style (`feat(scheduler): …`); end the commit body with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

## File Structure

- `src-tauri/Cargo.toml` — add `fsrs` dependency.
- `src-tauri/src/fsrs_sched.rs` — **new**: DTOs, `fsrs_next_states` command, `cargo test` module.
- `src-tauri/src/lib.rs` — register `mod fsrs_sched;` + the command.
- `src/domain/scheduler/Scheduler.ts` — new interface shape (`initial` sync, `previewNextStates` async).
- `src/domain/scheduler/tauriFsrs.ts` — **new**: `TauriFsrsScheduler`, `DeckFsrsParams`, `DEFAULT_DECK_FSRS_PARAMS`, pure `mapNextStates`.
- `src/domain/scheduler/fakeScheduler.ts` — **new**: deterministic `FakeScheduler`.
- `src/domain/scheduler/fakeScheduler.test.ts` — **new**: unit tests for the fake.
- `src/domain/scheduler/tauriFsrs.test.ts` — **new**: unit test for pure `mapNextStates`.
- `src/domain/scheduler/index.ts` — `getScheduler()` no-arg, `isTauri()` selection.
- `src/domain/scheduler/fsrs.ts`, `src/domain/scheduler/fsrs.test.ts` — **delete**.
- `src/data/dexie/db.ts`, `src/data/dexie/DexieStorage.ts`, `src/data/backup.ts` — update three `initial()` call sites.
- `src/features/review/ReviewPage.tsx` — async reveal + cached grading.
- `src/features/review/GradeButtons.tsx` — render cached previews.
- `src/features/review/reviewCycle.test.ts` — update to new async API.
- `package.json` — drop `ts-fsrs`.

---

### Task 1: Rust `fsrs_next_states` command (cargo-tested)

Rust-only; the TS side is untouched, so the app still builds. Delivers the engine with `cargo test` coverage.

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/fsrs_sched.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces (Tauri command, called by TS in Task 2):
  - command name `fsrs_next_states`
  - args (camelCase from JS): `state: FsrsStateDto`, `now: i64`, `params: DeckFsrsParams`
  - `FsrsStateDto { stability, difficulty, reps, lapses, state, lastReview: number|null, due }`
  - `DeckFsrsParams { desiredRetention, maximumInterval, weights: number[]|null }`
  - returns `NextStatesDto { again, hard, good, easy: FsrsStateDto }`

- [ ] **Step 1: Add the `fsrs` crate**

Run (in `src-tauri/`):

```bash
cd src-tauri && cargo add fsrs
```

Expected: `Cargo.toml` gains a `fsrs = "<version>"` line under `[dependencies]` and `cargo` resolves it. (The crate must be FSRS-6 — `DEFAULT_PARAMETERS` has 21 elements. If `cargo add` picks an older 0.x, run `cargo add fsrs@latest`.)

- [ ] **Step 2: Write the failing command module with tests**

Create `src-tauri/src/fsrs_sched.rs`:

```rust
use fsrs::{ItemState, MemoryState, FSRS};
use serde::{Deserialize, Serialize};

const MS_PER_DAY: i64 = 86_400_000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsrsStateDto {
    pub stability: f32,
    pub difficulty: f32,
    pub reps: u32,
    pub lapses: u32,
    pub state: u8, // 0 New / 1 Learning / 2 Review / 3 Relearning
    pub last_review: Option<i64>,
    pub due: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckFsrsParams {
    pub desired_retention: f32,
    pub maximum_interval: u32,
    pub weights: Option<Vec<f32>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStatesDto {
    pub again: FsrsStateDto,
    pub hard: FsrsStateDto,
    pub good: FsrsStateDto,
    pub easy: FsrsStateDto,
}

/// Build the next stored state for one grade from fsrs-rs output.
fn transition(prev: &FsrsStateDto, item: &ItemState, is_again: bool, now: i64, max_interval: u32) -> FsrsStateDto {
    let interval_days = (item.interval.round() as i64).clamp(1, max_interval as i64);
    let lapsed = is_again && prev.state == 2;
    FsrsStateDto {
        stability: item.memory.stability,
        difficulty: item.memory.difficulty,
        reps: prev.reps + 1,
        lapses: prev.lapses + if lapsed { 1 } else { 0 },
        state: 2,
        last_review: Some(now),
        due: now + interval_days * MS_PER_DAY,
    }
}

#[tauri::command]
pub fn fsrs_next_states(state: FsrsStateDto, now: i64, params: DeckFsrsParams) -> Result<NextStatesDto, String> {
    // None weights -> FSRS-6 defaults. next_states requires real params, so pass them explicitly.
    let weights = params.weights.unwrap_or_else(|| fsrs::DEFAULT_PARAMETERS.to_vec());
    let fsrs = FSRS::new(Some(&weights)).map_err(|e| e.to_string())?;

    let days_elapsed = match state.last_review {
        Some(t) => ((now - t).max(0) / MS_PER_DAY) as u32,
        None => 0,
    };
    let current = if state.reps == 0 {
        None
    } else {
        Some(MemoryState { stability: state.stability, difficulty: state.difficulty })
    };

    let ns = fsrs
        .next_states(current, params.desired_retention, days_elapsed)
        .map_err(|e| e.to_string())?;

    let max = params.maximum_interval;
    Ok(NextStatesDto {
        again: transition(&state, &ns.again, true, now, max),
        hard: transition(&state, &ns.hard, false, now, max),
        good: transition(&state, &ns.good, false, now, max),
        easy: transition(&state, &ns.easy, false, now, max),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn params() -> DeckFsrsParams {
        DeckFsrsParams { desired_retention: 0.9, maximum_interval: 36500, weights: None }
    }
    fn new_card(now: i64) -> FsrsStateDto {
        FsrsStateDto { stability: 0.0, difficulty: 0.0, reps: 0, lapses: 0, state: 0, last_review: None, due: now }
    }

    #[test]
    fn new_card_first_review_produces_four_future_states() {
        let ns = fsrs_next_states(new_card(NOW), NOW, params()).unwrap();
        for s in [&ns.again, &ns.hard, &ns.good, &ns.easy] {
            assert_eq!(s.reps, 1);
            assert_eq!(s.state, 2);
            assert_eq!(s.last_review, Some(NOW));
            assert!(s.due >= NOW + MS_PER_DAY); // interval clamped to >= 1 day
        }
    }

    #[test]
    fn intervals_are_ordered() {
        let ns = fsrs_next_states(new_card(NOW), NOW, params()).unwrap();
        assert!(ns.again.due <= ns.hard.due);
        assert!(ns.hard.due <= ns.good.due);
        assert!(ns.good.due <= ns.easy.due);
    }

    #[test]
    fn again_on_reviewed_card_counts_a_lapse() {
        let first = fsrs_next_states(new_card(NOW), NOW, params()).unwrap().good; // state now 2
        let later = first.due;
        let ns = fsrs_next_states(first, later, params()).unwrap();
        assert_eq!(ns.again.lapses, 1);
        assert_eq!(ns.good.lapses, 0);
    }

    #[test]
    fn maximum_interval_clamps_due() {
        let p = DeckFsrsParams { desired_retention: 0.9, maximum_interval: 5, weights: None };
        let ns = fsrs_next_states(new_card(NOW), NOW, p).unwrap();
        for s in [&ns.again, &ns.hard, &ns.good, &ns.easy] {
            assert!(s.due <= NOW + 5 * MS_PER_DAY);
        }
    }

    #[test]
    fn due_is_now_plus_whole_days() {
        let ns = fsrs_next_states(new_card(NOW), NOW, params()).unwrap();
        let days = (ns.good.due - NOW) / MS_PER_DAY;
        assert_eq!(ns.good.due, NOW + days * MS_PER_DAY);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail (module not wired)**

Run:

```bash
cd src-tauri && cargo test fsrs_sched 2>&1 | tail -20
```

Expected: FAIL — `fsrs_sched.rs` is not yet a module of the crate, so `cargo test` reports it is not compiled / the tests are not found. (If `cargo add` chose a non-FSRS-6 version, you'll instead get a compile error on `fsrs::DEFAULT_PARAMETERS` length or `ItemState`/`MemoryState`/`next_states` not resolving — fix by upgrading the crate, then continue.)

- [ ] **Step 4: Wire the module + command into `lib.rs`**

In `src-tauri/src/lib.rs`, add `mod fsrs_sched;` next to `mod git;`, and add the command to the handler list:

```rust
mod fsrs_sched;
mod git;
```

```rust
        .invoke_handler(tauri::generate_handler![
            git::git_is_cloned,
            git::git_clone,
            git::git_fetch_reset,
            git::git_read_files,
            git::git_write_files,
            git::git_read_assets,
            git::git_write_assets,
            git::git_commit_push,
            fsrs_sched::fsrs_next_states,
        ])
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cd src-tauri && cargo test fsrs_sched 2>&1 | tail -20
```

Expected: PASS — `5 passed`. (First run compiles the `fsrs` crate; it may take a minute.)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/fsrs_sched.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(scheduler): add fsrs-rs next_states Tauri command

Stateless Rust command computing all four FSRS grade outcomes for a card,
owned per-card state transition (reps/lapses/state/due), behind cargo tests.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: TypeScript seam swap to async Rust scheduler

The interface change ripples through every caller (TypeScript couples the seam to its consumers), so this is one cohesive task that keeps the build green at commit. The FSRS algorithm now lives in Rust; TS keeps **wiring** tests on a deterministic fake.

**Files:**
- Modify: `src/domain/scheduler/Scheduler.ts`
- Create: `src/domain/scheduler/tauriFsrs.ts`, `src/domain/scheduler/tauriFsrs.test.ts`
- Create: `src/domain/scheduler/fakeScheduler.ts`, `src/domain/scheduler/fakeScheduler.test.ts`
- Modify: `src/domain/scheduler/index.ts`
- Delete: `src/domain/scheduler/fsrs.ts`, `src/domain/scheduler/fsrs.test.ts`
- Modify: `src/data/dexie/db.ts:58`, `src/data/dexie/DexieStorage.ts:53`, `src/data/backup.ts:162`
- Modify: `src/features/review/ReviewPage.tsx`, `src/features/review/GradeButtons.tsx`
- Modify: `src/features/review/reviewCycle.test.ts`
- Modify: `package.json`

**Interfaces:**
- Consumes (from Task 1): the `fsrs_next_states` Tauri command and its DTO shapes.
- Produces (used by Task 3 and later sub-projects):
  - `interface Scheduler { initial(now: number): SchedulingState; previewNextStates(state: SchedulingState, now: number): Promise<Record<Grade, SchedulingState>> }`
  - `getScheduler(): Scheduler` (no argument)
  - `DeckFsrsParams { desiredRetention: number; maximumInterval: number; weights: number[] | null }` and `DEFAULT_DECK_FSRS_PARAMS`
  - `mapNextStates(dto): Record<Grade, SchedulingState>` (pure)
  - `GradeButtons({ nexts: Record<Grade, SchedulingState>; now: number; onGrade: (g: Grade) => void })`

- [ ] **Step 1: Write the failing fake-scheduler test**

Create `src/domain/scheduler/fakeScheduler.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import type { SchedulingState } from '../models'
import { FakeScheduler } from './fakeScheduler'

const now = 1_700_000_000_000
const s = new FakeScheduler()

describe('FakeScheduler.initial', () => {
  it('makes a new card due now, unreviewed, kind fsrs', () => {
    const c = s.initial(now)
    expect(c).toEqual({ kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, lastReview: null, due: now })
  })
})

describe('FakeScheduler.previewNextStates', () => {
  it('returns four ascending future states and bumps reps', async () => {
    const n = await s.previewNextStates(s.initial(now), now)
    expect(n.again.reps).toBe(1)
    expect(n.again.due).toBeLessThan(n.hard.due)
    expect(n.hard.due).toBeLessThan(n.good.due)
    expect(n.good.due).toBeLessThan(n.easy.due)
  })

  it('counts a lapse only when failing a Review-state card', async () => {
    const reviewed: SchedulingState = { ...s.initial(now), reps: 1, state: 2 }
    const fromReview = await s.previewNextStates(reviewed, now)
    expect(fromReview.again.lapses).toBe(1)
    const fromNew = await s.previewNextStates(s.initial(now), now)
    expect(fromNew.again.lapses).toBe(0)
  })

  it('is deterministic', async () => {
    const a = await s.previewNextStates(s.initial(now), now)
    const b = await s.previewNextStates(s.initial(now), now)
    expect(a).toEqual(b)
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run src/domain/scheduler/fakeScheduler.test.ts`
Expected: FAIL — `Cannot find module './fakeScheduler'`.

- [ ] **Step 3: Write the new `Scheduler` interface**

Replace `src/domain/scheduler/Scheduler.ts` entirely with:

```ts
import type { Grade, SchedulingState } from '../models'

/**
 * A spaced-repetition scheduling algorithm.
 *
 * `initial` is pure and synchronous (a brand-new card needs no algorithm).
 * `previewNextStates` returns all four grade outcomes at once — the real
 * implementation crosses into Rust, so it is async.
 */
export interface Scheduler {
  /** Scheduling state for a brand-new card (immediately due). */
  initial(now: number): SchedulingState
  /** All four grade outcomes for the next review. */
  previewNextStates(state: SchedulingState, now: number): Promise<Record<Grade, SchedulingState>>
}
```

- [ ] **Step 4: Implement `FakeScheduler`**

Create `src/domain/scheduler/fakeScheduler.ts`:

```ts
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { Scheduler } from './Scheduler'

const MS_PER_DAY = 86_400_000
const OFFSET_DAYS: Record<Grade, number> = { again: 0, hard: 1, good: 3, easy: 7 }

function emptyCard(now: number): FSRSState {
  return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, lastReview: null, due: now }
}

/** Deterministic, non-FSRS stand-in for tests and non-Tauri dev. Real FSRS math
 *  is in Rust (cargo-tested); this only needs to exercise the wiring. */
export class FakeScheduler implements Scheduler {
  initial(now: number): SchedulingState {
    return emptyCard(now)
  }

  async previewNextStates(state: SchedulingState, now: number): Promise<Record<Grade, SchedulingState>> {
    if (state.kind !== 'fsrs') throw new Error('expected fsrs state')
    const make = (g: Grade): FSRSState => ({
      kind: 'fsrs',
      stability: state.stability,
      difficulty: state.difficulty,
      reps: state.reps + 1,
      lapses: state.lapses + (g === 'again' && state.state === 2 ? 1 : 0),
      state: 2,
      lastReview: now,
      due: now + OFFSET_DAYS[g] * MS_PER_DAY,
    })
    return { again: make('again'), hard: make('hard'), good: make('good'), easy: make('easy') }
  }
}
```

- [ ] **Step 5: Run the fake test to verify it passes**

Run: `npx vitest run src/domain/scheduler/fakeScheduler.test.ts`
Expected: PASS — 4 tests.

- [ ] **Step 6: Write the failing `mapNextStates` test**

Create `src/domain/scheduler/tauriFsrs.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mapNextStates } from './tauriFsrs'

describe('mapNextStates', () => {
  it('tags each DTO branch with kind: fsrs and preserves fields', () => {
    const branch = { stability: 3.2, difficulty: 5.1, reps: 2, lapses: 1, state: 2, lastReview: 100, due: 200 }
    const dto = { again: branch, hard: branch, good: branch, easy: branch }
    const out = mapNextStates(dto)
    expect(out.good).toEqual({ kind: 'fsrs', ...branch })
    expect(out.again.kind).toBe('fsrs')
  })
})
```

- [ ] **Step 7: Run to verify it fails**

Run: `npx vitest run src/domain/scheduler/tauriFsrs.test.ts`
Expected: FAIL — `Cannot find module './tauriFsrs'`.

- [ ] **Step 8: Implement `TauriFsrsScheduler`**

Create `src/domain/scheduler/tauriFsrs.ts`:

```ts
import { invoke } from '@tauri-apps/api/core'
import type { Grade, FSRSState, SchedulingState } from '../models'
import type { Scheduler } from './Scheduler'

export interface DeckFsrsParams {
  desiredRetention: number
  maximumInterval: number
  weights: number[] | null
}

/** Defaults for sub-project #2. Sub-projects #1/#3 replace this with the deck's
 *  stored settings — the command already accepts a DeckFsrsParams argument. */
export const DEFAULT_DECK_FSRS_PARAMS: DeckFsrsParams = {
  desiredRetention: 0.9,
  maximumInterval: 36500,
  weights: null,
}

interface FsrsStateDto {
  stability: number
  difficulty: number
  reps: number
  lapses: number
  state: number
  lastReview: number | null
  due: number
}

interface NextStatesDto {
  again: FsrsStateDto
  hard: FsrsStateDto
  good: FsrsStateDto
  easy: FsrsStateDto
}

function toState(dto: FsrsStateDto): FSRSState {
  return { kind: 'fsrs', ...dto }
}

/** Pure DTO → domain mapping (unit-tested without Tauri). */
export function mapNextStates(dto: NextStatesDto): Record<Grade, SchedulingState> {
  return { again: toState(dto.again), hard: toState(dto.hard), good: toState(dto.good), easy: toState(dto.easy) }
}

export class TauriFsrsScheduler implements Scheduler {
  initial(now: number): SchedulingState {
    return { kind: 'fsrs', stability: 0, difficulty: 0, reps: 0, lapses: 0, state: 0, lastReview: null, due: now }
  }

  async previewNextStates(state: SchedulingState, now: number): Promise<Record<Grade, SchedulingState>> {
    const dto = await invoke<NextStatesDto>('fsrs_next_states', { state, now, params: DEFAULT_DECK_FSRS_PARAMS })
    return mapNextStates(dto)
  }
}
```

- [ ] **Step 9: Run the mapping test to verify it passes**

Run: `npx vitest run src/domain/scheduler/tauriFsrs.test.ts`
Expected: PASS — 1 test.

- [ ] **Step 10: Rewrite `index.ts` for no-arg `getScheduler()`**

Replace `src/domain/scheduler/index.ts` entirely with:

```ts
import { isTauri } from '@tauri-apps/api/core'
import type { Scheduler } from './Scheduler'
import { TauriFsrsScheduler } from './tauriFsrs'
import { FakeScheduler } from './fakeScheduler'

export type { Scheduler } from './Scheduler'

export const MS_PER_DAY = 86_400_000

const tauriScheduler = new TauriFsrsScheduler()
const fakeScheduler = new FakeScheduler()

/** The active scheduler: real fsrs-rs over Tauri in the app, deterministic fake
 *  in tests / non-Tauri dev. */
export function getScheduler(): Scheduler {
  return isTauri() ? tauriScheduler : fakeScheduler
}
```

- [ ] **Step 11: Delete the old ts-fsrs scheduler + its test**

```bash
git rm src/domain/scheduler/fsrs.ts src/domain/scheduler/fsrs.test.ts
```

- [ ] **Step 12: Update the three `initial()` call sites**

`src/data/dexie/DexieStorage.ts` — in `createCard`, the scheduler no longer needs the deck's `kind`, which makes both the `deck` fetch and the `kind` lookup dead. The current body is:

```ts
  async createCard(deckId: ID, front: string, back: string): Promise<Card> {
    const now = Date.now()
    const deck = await this.db.decks.get(deckId)
    const kind = deck?.schedulerKind ?? 'fsrs'
    const card: Card = {
      id: crypto.randomUUID(),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      scheduling: getScheduler(kind).initial(now),
    }
```

Replace it with (drop both `deck` and `kind` — they are now unused and `noUnusedLocals` would fail typecheck):

```ts
  async createCard(deckId: ID, front: string, back: string): Promise<Card> {
    const now = Date.now()
    const card: Card = {
      id: crypto.randomUUID(),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      scheduling: getScheduler().initial(now),
    }
```

`src/data/dexie/db.ts` — line ~58:

```ts
        const fresh = getScheduler().initial(now)
```

`src/data/backup.ts` — line ~162:

```ts
  return getScheduler().initial(now)
```

- [ ] **Step 13: Refactor `GradeButtons.tsx` to render cached previews**

Replace `src/features/review/GradeButtons.tsx` entirely with:

```tsx
import type { Grade, SchedulingState } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'

const GRADES: { grade: Grade; label: string; key: string }[] = [
  { grade: 'again', label: 'Again', key: '1' },
  { grade: 'hard', label: 'Hard', key: '2' },
  { grade: 'good', label: 'Good', key: '3' },
  { grade: 'easy', label: 'Easy', key: '4' },
]

/** Human-readable interval, e.g. 1d / 6d / 2mo / 1y. */
function formatInterval(days: number): string {
  if (days >= 365) return `${Math.round(days / 365)}y`
  if (days >= 30) return `${Math.round(days / 30)}mo`
  return `${days}d`
}

/** The four grading buttons, each previewing the interval it would schedule,
 *  from the pre-computed next-states. */
export function GradeButtons({
  nexts,
  now,
  onGrade,
}: {
  nexts: Record<Grade, SchedulingState>
  now: number
  onGrade: (grade: Grade) => void
}) {
  return (
    <div className="grade-row">
      {GRADES.map(({ grade, label, key }) => (
        <button
          key={grade}
          type="button"
          className={`grade grade-${grade}`}
          onClick={() => onGrade(grade)}
        >
          <span className="grade-label">{label}</span>
          <span className="grade-hint">
            {formatInterval(Math.max(1, Math.round((nexts[grade].due - now) / MS_PER_DAY)))}
          </span>
          <span className="kbd">{key}</span>
        </button>
      ))}
    </div>
  )
}
```

- [ ] **Step 14: Refactor `ReviewPage.tsx` for async reveal + cached grading**

In `src/features/review/ReviewPage.tsx`:

(a) Add `SchedulingState` to the model import on line 4:

```tsx
import type { Card, Grade, SchedulingState } from '../../domain/models'
```

(b) Add two state hooks after the `revealed` state (line ~18):

```tsx
  const [revealed, setRevealed] = useState(false)
  const [nexts, setNexts] = useState<Record<Grade, SchedulingState> | null>(null)
  const [revealedAt, setRevealedAt] = useState(0)
```

(c) Add a `reveal` callback and replace the `grade` callback (lines ~40-49):

```tsx
  const reveal = useCallback(() => {
    if (!current) return
    const now = Date.now()
    setRevealed(true)
    setRevealedAt(now)
    setNexts(null)
    void getScheduler()
      .previewNextStates(current.scheduling, now)
      .then(setNexts)
  }, [current])

  const grade = useCallback(
    async (g: Grade) => {
      if (!current || !nexts) return
      await storage.updateCard(current.id, { scheduling: nexts[g] })
      setIndex((i) => i + 1)
      setRevealed(false)
      setNexts(null)
    },
    [current, nexts, storage],
  )
```

(d) In the keydown effect, replace `setRevealed(true)` (line ~57) with `reveal()`, and add `reveal` to the effect dependency array (line ~70):

```tsx
        if (e.code === 'Space' || e.key === 'Enter') {
          e.preventDefault()
          reveal()
        }
```

```tsx
  }, [current, revealed, reveal, grade])
```

(e) Replace the two "Show answer" `onClick={() => setRevealed(true)}` handlers (lines ~133) with `onClick={reveal}`.

(f) Replace the `GradeButtons` usage (line ~149) with the cached form, gated on `nexts`:

```tsx
            {nexts && <GradeButtons nexts={nexts} now={revealedAt} onGrade={grade} />}
```

- [ ] **Step 15: Update `reviewCycle.test.ts` to the async API**

In `src/features/review/reviewCycle.test.ts`, replace the single remaining test body (and drop the now-redundant second `'grades an FSRS card'` test — there is only one algorithm path now):

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { getScheduler } from '../../domain/scheduler'

/**
 * Composes storage + scheduler exactly as ReviewPage does, guarding the
 * end-to-end loop the UI depends on. Uses the fake scheduler (non-Tauri).
 */
describe('review cycle', () => {
  const DB = 'rem-review-test'
  let storage: DexieStorage

  beforeEach(async () => {
    await Dexie.delete(DB)
    storage = new DexieStorage(new RemDB(DB))
  })

  it('grading "good" clears the card today and brings it back after the interval', async () => {
    const deck = await storage.createDeck('Deck')
    const card = await storage.createCard(deck.id, 'q', 'a')

    const t0 = Date.now()
    expect(await storage.countDue(deck.id, t0)).toBe(1)

    const nexts = await getScheduler().previewNextStates(card.scheduling, t0)
    await storage.updateCard(card.id, { scheduling: nexts.good })

    expect(await storage.countDue(deck.id, t0)).toBe(0)
    expect(nexts.good.due).toBeGreaterThan(t0)
    expect(await storage.countDue(deck.id, nexts.good.due)).toBe(1)
  })
})
```

- [ ] **Step 16: Remove `ts-fsrs` from `package.json`**

```bash
npm uninstall ts-fsrs
```

Expected: `ts-fsrs` removed from `dependencies`; `package-lock.json` updated. Confirm nothing imports it:

```bash
grep -rn "ts-fsrs" src && echo "FOUND (bad)" || echo "clean"
```

Expected: `clean`.

- [ ] **Step 17: Typecheck + run the full unit/browser suite**

Run:

```bash
npm run typecheck && npm test
```

Expected: typecheck passes (no `getScheduler` arity errors, no missing `next`); all Vitest projects pass — `fakeScheduler`, `tauriFsrs`, `reviewCycle`, `reveal.browser`, and `screens.browser` (review screens use retry-based queries, so the async reveal is tolerated; screenshots are captured, not compared).

- [ ] **Step 18: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(scheduler): route scheduling through async fsrs-rs seam

Replace ts-fsrs with an async Scheduler (initial() stays sync; previewNextStates()
crosses to the Rust fsrs_next_states command). getScheduler() picks TauriFsrsScheduler
in the app and a deterministic FakeScheduler in tests. ReviewPage computes the four
next-states once on reveal and caches them; GradeButtons renders the cached previews.
Drops ts-fsrs.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: End-to-end verification in the real app

The Tauri `invoke` path is the one thing automated tests don't exercise (they use the fake). Verify it in the running app.

**Files:** none (manual verification + optional fixes).

- [ ] **Step 1: Launch the native app**

Run:

```bash
npm run app:dev
```

Expected: the desktop window opens (not a browser). First launch recompiles Rust including the `fsrs` crate.

- [ ] **Step 2: Review a card through the Rust path**

In the app: create a deck, add a card, start review, click **Show answer**.

Expected:
- The four grade buttons appear with **ascending** intervals (Again ≤ Hard ≤ Good ≤ Easy), e.g. `1d / 1d / 3d / 8d` (values come from real FSRS, not the fake).
- Clicking a grade advances to the next card / "Review complete".
- Re-opening the deck shows the graded card is no longer due today.
- No errors in the dev console or terminal (a failed `invoke('fsrs_next_states', …)` would surface as a red console error).

- [ ] **Step 3: Confirm cargo + JS suites once more, then done**

Run:

```bash
cd src-tauri && cargo test fsrs_sched 2>&1 | tail -5 && cd .. && npm run typecheck && npm test
```

Expected: all green. Sub-project #2 complete; no commit needed unless Step 2 surfaced a fix (commit any fix with a `fix(scheduler): …` message).

---

## Notes for the executor

- **Per-deck params are intentionally hard-coded** to `DEFAULT_DECK_FSRS_PARAMS` here. Wiring real deck settings into `previewNextStates`/`fsrs_next_states` is sub-project #1/#3 — do not add it now (YAGNI).
- **No learning/relearning steps, leeches, daily caps, or burying** in this sub-project; the card always lands in `state = 2`. Those are sub-project #3.
- If `cargo add fsrs` resolves a version where `ItemState`, `MemoryState`, `next_states`, or `DEFAULT_PARAMETERS` differ, adjust the field/path references in Task 1 Step 2 to match the installed crate — the design (stateless command returning four `{memory, interval}` branches) is unchanged.
