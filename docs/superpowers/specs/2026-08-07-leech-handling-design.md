# Leech handling — design

_Date: 2026-08-07_

## Context

This is the executable part of review-queue slice **#3c**. The deck model already
stores `leechThreshold` and `leechAction`, and FSRS already increments
`FSRSState.lapses` when a Review-state card is graded Again. Nothing currently
acts on that count, cards cannot be suspended, and the `tag` action has no durable
representation.

The other half of #3c, **bury related new cards**, remains deferred. rem models
one independent `Card` per front/back pair and has no note/template/sibling
identity. Guessing that cards are related by text or tag would not be Anki-faithful;
adding an unused sibling identifier would be speculative. Burying should be
specified when rem gains a real note/template model.

## Goal

Honor each deck's leech policy when a Review card lapses:

- At the first qualifying lapse, add the system tag `leech`.
- With action `tag`, keep the card active and let it continue relearning.
- With action `suspend`, also suspend it and remove it from the current and future
  due queues.
- Show leech/suspended state in card management and let the user unsuspend a card.
- Preserve the metadata through restart, backup, and git sync.

Success means a Review card whose next lapse reaches a threshold of 1 is tagged;
under `suspend` it does not reappear for its short relearning step, is absent from
due counts after reopening, is visibly suspended, and can be restored from its
edit screen. Unsuspending keeps the `leech` tag and does not automatically trigger
the one-shot policy again.

## Decisions

- **Focused slice: leeches only.** Burying is deferred until related cards have a
  real domain identity.
- **Durable card metadata.** `Card` gains required `tags: string[]` and
  `suspended: boolean` fields. They are content state, so backup and sync include
  them. Only the system `leech` tag is created in this slice; general tag editing
  remains a separate feature.
- **One-shot trigger.** A card triggers only when all are true:
  1. the selected grade is `again`;
  2. the pre-grade state is Review (`state === 2`);
  3. the scheduler increased `lapses` and the new count is at least the deck's
     threshold; and
  4. the card does not already carry `leech`.
  This avoids repeatedly suspending a card after the user deliberately restores it.
- **Both actions tag.** `tag` adds `leech`; `suspend` adds `leech` and sets
  `suspended: true`.
- **Suspended cards remain stored and editable.** `listCards`/`getCard` include
  them; only `dueCards`/`countDue` exclude them. Total deck counts therefore still
  include suspended cards.
- **Unsuspend, don't reset.** Recovery sets only `suspended: false`; scheduling,
  lapse count, and `leech` tag remain intact.
- **Inline review notice.** A triggered action tells the user whether the card was
  tagged or suspended.

## Deep module and seams

### Pure leech-policy module

`src/features/review/leech.ts` is an in-process deep module. Its interface is one
pure function:

```ts
export interface LeechEffect {
  action: LeechAction
  tags: string[]
  suspended: boolean
}

export function leechEffect(
  card: Pick<Card, 'scheduling' | 'tags' | 'suspended'>,
  settings: Pick<DeckSettings, 'leechThreshold' | 'leechAction'>,
  grade: Grade,
  next: FSRSState,
): LeechEffect | null
```

The implementation hides threshold semantics, lapse-transition detection,
idempotency, and action mapping. `null` means the grade has no metadata effect.
The function never mutates `card.tags`.

### Storage seam

The existing `Storage` interface remains the persistence seam. `CardPatch` gains:

```ts
  tags?: string[]
  suspended?: boolean
```

No new storage methods are needed. `updateCard` already provides the required
atomic write of scheduling + leech metadata and stamps `updatedAt` for sync LWW.

### Review-session seam

`ReviewSession.grade` gains an options object:

```ts
grade(now, next, options?: { requeue?: boolean }): void
```

The default preserves current behavior. `ReviewPage` passes `requeue: false` only
when this grade suspends the card, preventing the new Relearning state from being
inserted into the current session.

## Data model and compatibility

```ts
export interface Card {
  // existing fields...
  tags: string[]
  suspended: boolean
  scheduling: SchedulingState
}
```

- New cards start with `tags: []`, `suspended: false`.
- Dexie **v9** backfills those defaults on existing cards; indexes are unchanged.
- Snapshot deserialization defaults absent fields for pre-#3c repositories.
- Backup parsing defaults absent fields while keeping backup format version 1.
- New snapshots/backups include both fields naturally.
- Card LWW merge behavior is unchanged because the full newest card record wins.

## Review flow

After scheduling outcomes are available and a grade is selected:

1. Compute `effect = leechEffect(current.card, current.settings, grade, next)`.
2. Persist one card patch containing `scheduling` and, when triggered, `tags` /
   `suspended`.
3. Preserve daily-cap accounting from the pre-grade state.
4. Grade the in-memory session with `requeue: effect?.suspended !== true`.
5. Show an inline tagged/suspended notice and advance.

A Review Again still counts toward `reviewsDone`, including when it suspends the
card. Learning/Relearning Again does not create an additional leech event because
FSRS does not increment lapses there and the pre-state is not Review.

## Card-management behavior

- Deck detail status priority: `suspended` → `leech` → existing scheduling status.
- The edit screen displays durable system tags.
- A suspended card gets an **Unsuspend card** action. It updates storage
  immediately and leaves the editor open with an **Active** indication.
- No general tag editor or manual suspend action is added in this slice.

## Verification seams (confirmed)

1. **Pure policy:** threshold, non-Again/non-Review exclusions, tag action,
   suspend action, idempotency, and input immutability.
2. **Storage:** defaults, metadata patch round-trip, suspended due filtering, v9
   migration, old/new backup and snapshot compatibility.
3. **Browser:** Review Again at threshold applies the action and removes a
   suspended card from the session; deck/editor surfaces the state; editor
   unsuspends and restores due eligibility.

Full gates: `npm test`, `npm run typecheck`, `cargo fmt --all --check`,
`cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.

## Out of scope

- Burying and any sibling/note/template model.
- General-purpose tag creation/removal UI.
- Manual suspension.
- Review history / FSRS weight optimization.
- Repeated leech notifications at later lapse multiples.
