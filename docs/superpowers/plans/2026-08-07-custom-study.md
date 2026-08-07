# Custom study — implementation plan

_Date: 2026-08-07_

**Goal:** Activate the four Deck options custom-study presets as temporary single-deck sessions.

**Architecture:** A pure selector converts route requests into an ordered card set. ReviewPage remains
the session driver; SessionCard marks initially forced cards, and preview mode uses a no-write advance
path. `Card.lastAgainAt` supplies exact forgotten-card selection without a review-log subsystem.

## 1. Card metadata

- Add `lastAgainAt` to `Card`, `CardPatch`, card creation/import, backup, and sync records.
- Add Dexie v10 migration backfilling `null`.
- Update backup/snapshot/migration tests for round-trip and backward compatibility.
- Verify with focused data tests and typecheck.

## 2. Pure custom-study selection

- Add `src/features/review/customStudy.ts` and tests.
- Define validated modes/requests and mode UI metadata.
- Select ahead, extra-new, forgotten, and recent-new cards per the design.
- Exclude suspended cards and preserve insertion-order behavior for extra new cards.
- Verify focused unit tests.

## 3. Session eligibility

- Add optional `forceDue` to `SessionCard`.
- Let forced cards enter once before their due date.
- Clear the force marker when a short-step result is requeued.
- Add a no-requeue `complete()` operation for preview.
- Verify focused session tests.

## 4. Activate Deck options UI

- Track selected mode and mode-specific amount in `DeckSettingsPage`.
- Enable preset buttons, selected styling, contextual stepper label/units, and Start navigation.
- Replace the inert browser test with selection/amount/navigation coverage.
- Add only the CSS needed for selected state and compact control copy.

## 5. Drive custom sessions

- Parse custom query params in `ReviewPage`.
- Load all deck cards and use the selector for custom sessions; retain the ordinary path unchanged.
- Mark custom cards forced for initial eligibility.
- On every Again grade, persist `lastAgainAt` with the existing card update.
- Preview reveal skips scheduler work and advances without persistence/counters.
- Add custom empty/completion/back-link copy and browser coverage.

## 6. Close out

- Run `npm test` and `npm run build`.
- Run `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and
  `cargo test` in `src-tauri`.
- Update the continuation ledger to mark custom study implemented and identify FSRS optimization as
  the next actionable slice.
