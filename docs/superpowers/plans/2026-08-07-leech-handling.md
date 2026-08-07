# Leech Handling Implementation Plan

**Goal:** Enforce `leechThreshold` / `leechAction` with durable tags and suspension,
while deferring burying until rem has a real sibling model.

**Spec:** `docs/superpowers/specs/2026-08-07-leech-handling-design.md`

**Method:** Vertical TDD slices at the three confirmed seams: pure leech policy,
`Storage`, and browser behavior. Never stage or modify the untracked `features.md`.

## Task 1 — Card metadata and due filtering

- [ ] Add a failing `DexieStorage` test proving new-card defaults, metadata patch
      round-trip, and suspended-card exclusion from `dueCards`/`countDue`.
- [ ] Add required `Card.tags` / `Card.suspended`, extend `CardPatch`, seed defaults,
      and filter the due queue.
- [ ] Run the focused storage test.

## Task 2 — v9 and serialized-data compatibility

- [ ] Add a failing Dexie v9 migration test for pre-v9 cards.
- [ ] Add the v9 backfill (`tags: []`, `suspended: false`).
- [ ] Add failing backup and snapshot tests for metadata round-trip and old-data
      defaults.
- [ ] Extend backup/import and snapshot normalization minimally.
- [ ] Run focused migration/backup/snapshot tests.

## Task 3 — Pure leech policy

- [ ] Add `leech.test.ts` through the public `leechEffect` interface.
- [ ] Cover: below threshold, wrong grade/state, tag, suspend, idempotency, and no
      input mutation.
- [ ] Implement `leech.ts` minimally and run its test.

## Task 4 — Review integration

- [ ] Add a session test proving `requeue: false` discards a short Relearning step.
- [ ] Extend `ReviewSession.grade` with the defaulted options object.
- [ ] Add a browser test: Review Again at threshold tags+suspends, shows notice,
      completes rather than reappears, increments the review counter, and is absent
      after reopening.
- [ ] Integrate `leechEffect` into the existing guarded grade transaction.
- [ ] Add the tag-action browser case (tagged but not suspended; relearning remains).

## Task 5 — Visibility and recovery

- [ ] Extend card-status tests for `suspended` and `leech` priority.
- [ ] Update Deck detail status rendering and styles.
- [ ] Add a Card editor browser test that displays the leech state, unsuspends,
      preserves the tag, and restores due eligibility.
- [ ] Implement the smallest editor status/Unsuspend UI.

## Task 6 — Type propagation and full verification

- [ ] Update typed Card/CardRecord/CardBackup fixtures with metadata defaults.
- [ ] Run `npm run typecheck` and fix only #3c-induced errors.
- [ ] Run `npm test`.
- [ ] Run Rust format, Clippy, and test gates.
- [ ] Update the continuation note: leech handling done; burying explicitly waits
      for the note/template model.
- [ ] Review the final diff for local-only scope and verify `features.md` untouched.
