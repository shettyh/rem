# Per-deck FSRS weight optimization — design

_Date: 2026-08-07_

## Context

rem schedules with fsrs-rs 6.6.1 and already carries a dormant
`DeckFsrsParams.weights: number[] | null` field across the TypeScript/Rust boundary. The missing
pieces are durable per-grade history, optimizer invocation, and per-deck weight persistence.

Research: `docs/research/2026-08-07-fsrs-rs-optimization.md`.

## Goal

Record trustworthy FSRS review histories and let a user optimize or reset the parameters for one
deck from Deck options. Optimized weights affect future scheduling calls without rewriting current
card states or due dates.

Success =
- FSRS-effective grades create immutable, synced review logs atomically with the scheduling update.
- Fixed learning/relearning step clicks do not pollute optimizer input.
- Deck options shows recorded-history status and enables Optimize once a card has a delayed review.
- Rust converts timestamped per-card histories into fsrs-rs training items off the command thread.
- Successful weights persist on the deck, survive backup/sync, and are sent into every later
  `fsrs_next_states` call; Reset restores FSRS-6 defaults (`null`).

## Decisions

### Per deck, manual optimization

Weights live in `DeckSettings.fsrsWeights: number[] | null`. This matches rem's existing settings
scope and Anki's guidance that dissimilar material may deserve separate parameter presets. There is
no global preset-sharing model yet.

Optimization is explicit, not automatic. The UI can show the amount of recorded history, but does
not schedule monthly runs, trigger from review, or persist progress.

### Log only events that update FSRS memory

A UI grade is an optimizer event iff the selected next state has `next.reps > previous.reps`.
This includes:
- new-card graduation;
- Review grades, including a lapse into Relearning;
- Relearning graduation when the runtime calls FSRS again.

It excludes pure fixed learning/relearning step transitions and non-rescheduling preview. This
keeps training aligned with the runtime's accepted choice to let TypeScript own fixed steps.

### Atomic review commit

Add a storage operation that commits one grade transaction:

```ts
interface ReviewCommit {
  cardId: ID
  deckId: ID
  patch: CardPatch
  reviewedAt: number
  fsrsGrade?: Grade
  daily?: { day: string; field: 'newIntroduced' | 'reviewsDone' }
}

Storage.commitReview(commit): Promise<ReviewLog | null>
```

Dexie performs the card patch, optional daily-counter increment, and optional immutable log insert
in one transaction. Existing `updateCard` / `bumpDailyStat` stay for non-review call sites and tests,
but ReviewPage uses only `commitReview`.

### Review-log model and lifecycle

```ts
interface ReviewLog {
  id: ID
  deckId: ID
  cardId: ID
  reviewedAt: number
  grade: Grade
}
```

Dexie **v11** adds `reviewLogs: 'id, deckId, cardId, reviewedAt'` and backfills
`DeckSettings.fsrsWeights = null` on existing decks.

Logs are immutable. Deleting a card/deck cascades its logs. There is no individual-log delete API,
so sync needs no review-log tombstones.

### Backup and sync are full-fidelity

Backups nest review events under each card (timestamp + grade, IDs omitted); imports regenerate log
IDs and remap to regenerated card/deck IDs. Old v1 backups default to no logs and null weights.

Repo snapshots gain top-level `reviewLogs`; each deck JSON file writes an optional `reviewLogs`
array. Merge unions logs by UUID, then retains only logs whose card survived and whose deck matches
the surviving card. `DbOps` upserts/deletes logs so a card tombstone also removes its history. Old
sync files normalize missing logs to an empty array and missing weights to null.

The sync format version remains 1 because all additions are optional and both readers are backward
compatible.

### Optimizer boundary

TypeScript groups logs by card and maps grades to ratings 1–4 for this DTO:

```ts
interface FsrsReviewHistory {
  reviews: Array<{ reviewedAt: number; rating: number }>
}
```

`fsrs_optimize(histories, numRelearningSteps)` is an async Tauri command. It dispatches CPU work via
`spawn_blocking`. Rust sorts each history, computes whole-day deltas (first = 0), builds every
cumulative prefix containing at least one `delta_t > 0`, assigns aligned numeric card IDs, and calls:

```rust
compute_parameters(ComputeParametersInput {
    train_set,
    card_ids: Some(card_ids),
    enable_short_term: false,
    num_relearning_steps: Some(num_relearning_steps),
    ..Default::default()
})
```

The crate owns small-data fallback behavior and weight validation.

### Deck options UI

Add an **FSRS parameters** row below Desired retention:
- status: `Default parameters` or `Optimized parameters`;
- history: `N recorded FSRS reviews`;
- **Optimize** button, disabled until at least one card history spans one whole day;
- **Reset** shown for personalized weights;
- inline `Optimizing…`, success, and recoverable error states.

An optimization result updates the local form settings and persists the whole settings object.
Reset stores `null`. Existing card memory/due values remain unchanged.

## Out of scope

- Historical backfill from aggregate card state (impossible to reconstruct faithfully).
- Automatic re-optimization, reminders, progress percentage, or cancellation.
- Sharing one parameter preset across multiple decks.
- Optimal-retention calculation.
- Recomputing current memory states/due dates after changing weights.
- Importing Anki review history.

## Verification

1. Storage tests: atomic card/counter/log commit, pure-step no-log, deletion cascade.
2. Migration/backup/snapshot/merge tests: weights + logs, old defaults, ID remap, deletion filtering.
3. Pure TS tests: grouping/order/rating map, delayed-history eligibility, settings-to-params weights.
4. Browser tests: Deck options status/disabled state, Optimize persistence (with mocked command), Reset;
   ReviewPage records only FSRS-effective grades and double-grade guard still creates one log.
5. Rust tests: timestamp-to-prefix conversion, ratings/deltas/card IDs, default result on a small valid
   dataset, invalid input propagation; command registered.
6. Full TypeScript build/tests and Rust fmt/Clippy/tests.
