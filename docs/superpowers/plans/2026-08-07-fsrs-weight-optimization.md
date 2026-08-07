# Per-deck FSRS weight optimization — implementation plan

_Date: 2026-08-07_

**Goal:** Capture FSRS-effective review history and train per-deck FSRS-6 weights from Deck options.

**Spec:** `docs/superpowers/specs/2026-08-07-fsrs-weight-optimization-design.md`

**Method:** Vertical TDD through storage, serialization/sync, optimizer boundaries, then UI wiring.
Never stage or modify the user's untracked `features.md`.

## 1. Review-log model and atomic storage commit

- Add failing Dexie tests for `commitReview`: card patch + daily counter + optional log in one commit.
- Add `ReviewLog` / `ReviewCommit`, Storage methods, Dexie v11 table, and deck weight backfill.
- Add list ordering and card/deck deletion-cascade tests.
- Verify focused storage/migration tests and typecheck.

## 2. Backup and sync compatibility

- Add failing backup tests for nested review histories, old-file defaults, and import ID remapping.
- Extend backup collection/parser/import transaction minimally.
- Add failing snapshot round-trip/default tests and merge tests for log union + dead-card filtering.
- Extend `RepoSnapshot`, deck-file serialization, merge `DbOps`, and Dexie apply/export.
- Verify focused data tests.

## 3. Record trustworthy review events

- Add browser tests proving an FSRS-effective grade creates one log and a fixed learning step creates none.
- Update the double-grade test to assert one atomic commit/log.
- Replace ReviewPage's separate card/counter writes with `commitReview` and the `next.reps > previous.reps` rule.
- Verify review browser tests and existing cap/leech behavior.

## 4. Rust optimizer command

- Add Rust tests for history conversion: chronological sorting, rating/delta mapping, cumulative prefixes,
  aligned card IDs, and same-day-only filtering.
- Add a small-data optimizer test (fast default fallback).
- Implement `fsrs_optimize` core + async `spawn_blocking` command and register it.
- Verify cargo fmt, Clippy, and tests.

## 5. TypeScript optimizer seam

- Add pure tests for grouping logs, grade-to-rating mapping, and delayed-history eligibility.
- Add `FsrsOptimizer` with Tauri and deterministic fake implementations, mirroring the scheduler seam.
- Thread `DeckSettings.fsrsWeights` through `settingsToParams`; update its unit test.

## 6. Deck options UI

- Add browser tests for history count, disabled-until-delayed Optimize, successful persistence, Reset,
  loading guard, and recoverable error.
- Add the FSRS parameters row and minimal status styling using existing deck-settings primitives.
- Persist optimized/reset weights through the existing whole-settings update.

## 7. Close out

- Run `npm test` and `npm run build`.
- Run Rust fmt, Clippy, and tests.
- Update the continuation ledger: #5 implemented; identify the next product slice from `features.md`/
  roadmap rather than inventing another scheduler sub-project.
- Review the final diff and verify `features.md` remains untouched.
