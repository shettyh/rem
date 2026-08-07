# fsrs-rs 6.6.1 optimization research

_Date: 2026-08-07_

## Question

What review-history shape, optimizer call, and product constraints should rem use to train
per-deck FSRS-6 weights with its pinned `fsrs = 6.6.1` dependency?

## Findings

### Input is chronological per-card grades plus elapsed whole days

`FSRSReview` contains `rating: u32` (1–4) and `delta_t: u32`; its documentation explicitly
requires the first review's `delta_t` to be zero. `FSRSItem` contains a review prefix for one card.
The crate's optimizer example converts timestamped histories into cumulative prefixes, then keeps
prefixes containing at least one long-term review (`delta_t > 0`).

Sources:
- [`FSRSReview` / `FSRSItem`, fsrs-rs 6.6.1 source](https://docs.rs/fsrs/6.6.1/src/fsrs/dataset.rs.html#13-38)
- [`examples/optimize.rs`, v6.6.1](https://github.com/open-spaced-repetition/fsrs-rs/blob/v6.6.1/examples/optimize.rs)

Implication for rem: persist immutable `(cardId, deckId, reviewedAt, grade)` events. At training
time, group by card, sort chronologically, map Again/Hard/Good/Easy to 1/2/3/4, and compute
`floor((current - previous) / 86_400_000)` days. Rust should build cumulative prefixes so the
conversion stays next to the optimizer API it serves.

### `compute_parameters()` supports card IDs and degrades safely on small datasets

`ComputeParametersInput` accepts `train_set`, optional card IDs aligned with the items, optional
progress, `enable_short_term`, relearning-step count, and optional training config. Card IDs let the
trainer group prefixes from the same card. The implementation validates non-empty items and ratings,
returns default parameters with fewer than 8 prepared items, returns initialized parameters below 64
items (or when only initialization data exists), and performs full training for larger datasets.

Sources:
- [`ComputeParametersInput`, fsrs-rs 6.6.1](https://docs.rs/fsrs/6.6.1/fsrs/struct.ComputeParametersInput.html)
- [`compute_parameters`, fsrs-rs 6.6.1 source](https://docs.rs/fsrs/6.6.1/src/fsrs/training.rs.html#261-389)

Implication for rem: optimization can be offered with a small history; the library already chooses
the appropriate fallback. The UI should still wait until at least one card has a delayed review,
because same-day-only histories produce no long-term training item.

### rem should disable short-term optimization

The pinned API exposes `enable_short_term`. rem deliberately implements fixed TypeScript
learning/relearning steps and only asks FSRS to update memory on graduation or a long-term review.
Training short-term weights from every UI click would therefore mismatch runtime behavior. Log only
grades whose persisted FSRS `reps` increases, and call the optimizer with
`enable_short_term: false`.

Sources:
- [`ComputeParametersInput.enable_short_term`](https://docs.rs/fsrs/6.6.1/fsrs/struct.ComputeParametersInput.html)
- rem's runtime step machine: `src/domain/scheduler/reviewScheduler.ts`
- rem's queue design: `docs/superpowers/specs/2026-07-02-anki-review-queue-steps-design.md`

### Optimization is user-triggered and need not run frequently

The Anki FSRS FAQ says modern optimizers can be used with any number of reviews and recommends
re-optimizing roughly monthly (or whenever the number of reviews doubles). There is no need to train
after every review.

Source:
- [Anki FSRS FAQ, Q4 and Q6](https://faqs.ankiweb.net/frequently-asked-questions-about-fsrs.html)

Implication for rem: add explicit Optimize and Reset actions in Deck options. Do not auto-train in
the review path. Since training is CPU-heavy, execute it through an asynchronous Tauri command using
a blocking worker rather than in the UI flow.

### Weights fit the existing scheduler contract

rem already sends `DeckFsrsParams.weights: number[] | null` into `fsrs_next_states`; Rust uses
FSRS-6 defaults for `None` and constructs `FSRS` from explicit weights otherwise. The missing pieces
are persistence and replacing the hard-coded `null` in `settingsToParams()`.

Source:
- `src/domain/scheduler/Scheduler.ts`
- `src/domain/scheduler/reviewScheduler.ts`
- `src-tauri/src/fsrs_sched.rs`

## Recommended design

1. Add a Dexie `reviewLogs` table and immutable `ReviewLog` domain record.
2. Append a log only when a grade increases `FSRSState.reps`.
3. Include logs in backup and git snapshot formats; merge by UUID and discard logs whose card/deck
   no longer survives.
4. Add `fsrsWeights: number[] | null` to `DeckSettings`; migrate/normalize older decks to `null`.
5. Add a Rust `fsrs_optimize` command that converts timestamp histories to prefix `FSRSItem`s and
   calls `compute_parameters` with aligned card IDs and `enable_short_term: false`.
6. Add an Optimize/Reset row to per-deck settings. Keep current scheduling states unchanged; new
   weights affect future grades only.

## Explicit limitations

- No pre-migration history can be reconstructed from current card state.
- No automatic schedule rescaling after weights change; weights apply on the next review.
- No training progress/cancellation UI in the first slice.
- Per-deck training does not yet let multiple similar decks share one parameter preset.
