# Custom study — design

_Date: 2026-08-07_

## Context

The Deck options screen already renders four inert Custom study presets. Daily caps,
learning/relearning queues, and leech handling are now shipped. This slice makes the
presets usable without introducing Anki's persistent filtered-deck model.

## Goal

A user can start a temporary, single-deck custom session from Deck options:

- **Study ahead** — review cards due in the next N days.
- **Increase new** — study up to N additional new cards beyond today's remaining new-card allowance.
- **Review forgotten** — restudy cards answered Again in the last N days.
- **Preview new** — preview new cards added in the last N days without changing scheduling or daily counters.

Suspended cards never enter any mode. A session exists only in the current route; it is
not persisted as a deck.

## Decisions

### Match Anki's preset meanings, but not filtered decks

The official Anki meanings distinguish day windows from card-count increases. The UI's
single N control therefore changes its label and units with the selected preset:

| Mode | N means | Default |
|---|---|---|
| Study ahead | days ahead | 1 day |
| Increase new | additional cards | 10 cards |
| Review forgotten | recent days | 1 day |
| Preview new | days since card creation | 1 day |

The selected preset and amount are route-local UI state. Start navigates to
`/decks/:deckId/study?custom=<mode>&amount=<N>`.

### Record only the fact needed for forgotten-card selection

`FSRSState.lastReview` cannot identify an Again grade: a later successful review overwrites
it. Add `Card.lastAgainAt: number | null`, set on every Again grade. It is part of the card
payload, so backup and git sync carry it using their existing card paths. Dexie v10 backfills
`null`; old backup/sync files normalize it to `null`.

A full immutable review log is deliberately deferred to FSRS weight optimization. Adding it
only to support one timestamp would introduce table lifecycle, sync, merge, and retention
policy that this feature does not need.

### Pure selection module

`src/features/review/customStudy.ts` owns request parsing, labels, and card selection.
Selection receives cards, current time, deck insertion order, and today's normal new slots.
It returns cards in deterministic study order (except the deck's explicit random-new policy):

- ahead: Review (`state === 2`), `now < due <= now + N days`, soonest first;
- increase-new: unsuspended due New cards in insertion order, skip the cards still admitted by
  the normal daily allowance, then take N;
- forgotten: `lastAgainAt >= now - N days`, most recently failed first;
- preview-new: New cards with `createdAt >= now - N days`, newest first.

### Force only selected cards into the initial queue

Custom cards can be not due. `SessionCard` gains an optional `forceDue` marker. `ReviewSession.next`
may choose a marked card regardless of its scheduling due date. When a graded card is reinserted for
a learning/relearning step, the marker is removed, so normal step timing and learn-ahead apply.

### Preview does not schedule

Preview uses the same question/answer surface, but after reveal shows **Next** instead of grade
buttons. Advancing removes the card from the in-memory session only. It does not call the scheduler,
write the card, apply leech policy, or bump daily counters.

The other three modes grade and reschedule normally. Existing daily-counter rules remain based on
the card's pre-grade state; an additional new card can therefore take `newIntroduced` above the
configured base cap, which keeps later normal sessions capped.

## UI states

- No preset is selected initially; Start is disabled.
- Selecting a preset highlights it, resets N to that mode's default, and enables Start.
- Custom sessions return to Deck options on End session, empty state, or completion.
- Empty custom selection says that no cards match instead of claiming that nothing is due.
- Preview completion reports cards previewed; graded modes keep the review count wording.

## Out of scope

- Persistent/renameable filtered decks.
- Study by card state or tag.
- Increasing the ordinary review cap (not one of the existing four comp presets).
- Full review history / FSRS weight optimization.
- Cross-deck custom study.

## Verification

1. Pure unit tests cover request parsing and all four selection modes, boundaries, cap skipping,
   random/sequential new ordering, and suspended exclusion.
2. Session tests cover forced initial eligibility and normal timing after requeue.
3. Browser tests cover preset selection/navigation, ahead grading, forgotten timestamp capture,
   preview-without-persistence, and mode-specific empty/completion copy.
4. Migration, backup, and snapshot tests cover `lastAgainAt` and old-data defaults.
5. `npm test`, `npm run build`, Rust fmt/clippy/tests.
