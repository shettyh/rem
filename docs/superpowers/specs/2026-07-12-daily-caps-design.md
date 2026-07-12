# Daily caps (`newPerDay` / `maxReviews`) — design

_Date: 2026-07-12_

## Context

Second slice of **sub-project #3** (the Anki-grade review queue). Slice **#3a**
(learning/relearning steps + real per-deck FSRS params + insertion order) is
shipped on `main` (PR #1, `8289d7d`); it added the dynamic session queue
(`src/features/review/session.ts`) and the step machine
(`src/domain/scheduler/reviewScheduler.ts`).

`DeckSettings` already carries `newPerDay` (default 20) and `maxReviews`
(default 200) — they landed with #1's Deck options screen but are **not
enforced**. This slice enforces them. The remaining #3 mechanisms — leech +
burying (**#3c**) and FSRS weight training (**#5**) — stay out of scope.

The gap: nothing records *how many new cards were introduced today* or *how many
reviews were done today*, per deck. `storage.dueCards` returns every card with
`due <= now`; `loadDueOverview` counts them but never caps. So the feature needs
a small amount of persistent, per-day state.

## Goal

Make the review queue honor each deck's daily caps like Anki:

- A deck introduces at most `newPerDay` new cards per day; once the day's new
  allowance is spent, no more New-state cards enter the session (in-progress
  learning cards still finish).
- A deck shows at most `maxReviews` Review-state cards per day; learning and
  relearning cards are **never** capped.
- Caps persist across app restarts and across multiple sessions on the same day,
  and reset at the local day rollover.
- Caps apply to both single-deck and "All decks" review, per deck.
- The Today screen's counts reflect the capped, studyable-today numbers.

Success =

- With `newPerDay = 2`, studying a fresh deck serves exactly 2 new cards; a
  third new card does not appear until the next day (learning re-shows of the
  first two still occur).
- With `maxReviews = 1`, only one Review-state card is served; a second due
  review waits for the next day, but a lapsed card's relearning steps still show.
- Ending and reopening a session the same day does not re-grant spent allowance.
- Pure cap/partition logic and the day-boundary helper are unit-tested; the
  counter store round-trips; a browser test proves the cap stops serving.

## Decisions (from brainstorming)

- **[D1] Lightweight daily counters, not a full review-log.** A new
  `dailyStats` table holds two integers per (deck, day). A full per-grade review
  log is deferred to **#5**, whose weight-training needs should drive its schema
  rather than us guessing it now. (YAGNI: caps need only two counts.)
- **[D2] "Day" = local calendar date, rollover at local midnight.** No
  configurable rollover hour (Anki's default is 4am; we have no such setting and
  #1 did not add one). Trade-off: cards studied 00:00–04:00 count toward the new
  day. A rollover-hour setting can be added later without reworking this.
- **[D3] What each cap counts (Anki semantics):**
  - `newPerDay` counts a card the first time it is graded **out of New**
    (pre-grade `state === 0`). Increments `newIntroduced` once; a card never
    re-enters New.
  - `maxReviews` counts grading a card whose **pre-grade `state === 2`
    (Review)**. Learning (1) and relearning (3) grades count toward neither cap;
    New grades count toward `newPerDay`, not this.
- **[D4] Gating happens at session build; counters make it durable.** The pure
  queue builder caps how many New and Review cards enter; in-progress
  learning/relearning cards are always included. `ReviewPage.grade` bumps the
  right counter (it knows the pre-grade state) alongside the existing persist.
- **[D5] Caps apply in single-deck and All-decks review**, per deck. Unlike
  #3a's insertion-order (single-deck only), a cap that silently did nothing in
  All-decks would be a confusing half-feature.
- **[D6] Today-screen counts reflect caps** so the numbers don't lie. Only
  `loadDueOverview`'s returned counts change; `DeckListPage`'s JSX is untouched.

## Data model

### New Dexie table `dailyStats` (v8, additive)

```ts
export interface DailyStat {
  id: string          // `${deckId}:${day}`, e.g. "abc123:2026-07-12"
  deckId: string
  day: string         // local calendar date, YYYY-MM-DD
  newIntroduced: number
  reviewsDone: number
}
```

`RemDB` gains `dailyStats!: EntityTable<DailyStat, 'id'>` and a **v8** version:

```ts
this.version(8).stores({
  decks: 'id, createdAt',
  cards: 'id, deckId, createdAt',
  tombstones: 'id, deletedAt',
  assets: 'hash',
  dailyStats: 'id, deckId, day',
})
```

Purely additive (new table only), mirroring the v3/v4 table additions — no
`.upgrade` callback, existing data untouched. Indexes `deckId`/`day` are declared
for completeness; reads are by primary `id`.

Counters are **local-only**: not written to `backup.ts`, `snapshot.ts`, or the
git sync. Multi-device cap coordination is deferred (ephemeral daily state;
belongs with a synced review-log, i.e. #5). `deleteDeck` also deletes that deck's
`dailyStats` rows (housekeeping; add `dailyStats` to its transaction).

## Day boundary (`src/features/review/day.ts`)

The rollover policy lives in one pure helper so storage stays day-agnostic (the
app decides "what day is now", storage just keys by the string it's given):

```ts
/** Local calendar date of a timestamp as YYYY-MM-DD (rollover at local midnight). */
export function localDay(ms: number): string
```

Implemented from a local `Date` (year/month/date, zero-padded). Unit-tested for
format and for the midnight boundary (23:59 vs 00:01 → adjacent day strings).

## Storage seam

Two methods on the `Storage` interface (and `DexieStorage`):

```ts
getDailyStat(deckId: ID, day: string): Promise<{ newIntroduced: number; reviewsDone: number }>
bumpDailyStat(deckId: ID, day: string, field: 'newIntroduced' | 'reviewsDone'): Promise<void>
```

- `getDailyStat` returns `{ newIntroduced: 0, reviewsDone: 0 }` when no row exists.
- `bumpDailyStat` upserts: read-or-init the row, `+1` the field, `put`, inside a
  `rw` transaction on `dailyStats` so concurrent grades don't lose an increment.

No other seam changes. `FakeScheduler` is unaffected (this is storage, not
scheduling).

## Cap gating (`src/features/review/session.ts`)

`buildSessionCards` gains a `caps` argument and partitions **three** ways:

```ts
export interface Caps { newSlots: number; reviewSlots: number }

export function buildSessionCards(
  cards: SessionCard[],
  order: InsertionOrder,
  caps: Caps,
): SessionCard[]
```

Let `newSlots' = max(0, caps.newSlots)`, `reviewSlots' = max(0, caps.reviewSlots)`.

- `newCards` = `state === 0`, ordered by `insertionOrder` (`sequential` →
  `createdAt`; `random` → `shuffle`), keep the first `newSlots'`.
- `reviewCards` = `state === 2`, ordered by `due` ascending, keep the first
  `reviewSlots'`.
- `inProgress` = `state === 1 || state === 3`, **all kept**, ordered by `due`.
- Result = `[...(inProgress ∪ keptReview) sorted by due, ...keptNew]` — same
  overall shape as #3a (rest-by-due, then new).

The runtime `ReviewSession` (re-insertion, learn-ahead) is unchanged: it operates
on whatever queue it's handed. Cards clamped out by a cap simply never enter,
and remain due for a later day via their persisted `due`.

## Overview capping (`src/features/review/dueOverview.ts`)

`loadDueOverview(storage, now)` computes each deck's caps and reuses
`buildSessionCards` to get that deck's capped, ordered queue:

```ts
const day = localDay(now)
// per deck:
const stat = await storage.getDailyStat(deck.id, day)
const caps = {
  newSlots: deck.settings.newPerDay - stat.newIntroduced,
  reviewSlots: deck.settings.maxReviews - stat.reviewsDone,
}
const sessionCards = dueList.map((card) => ({ card, settings: deck.settings }))
const capped = buildSessionCards(sessionCards, deck.settings.insertionOrder, caps)
```

From `capped` derive the counts:

- per-deck `newCount` = # capped cards with `state === 0`.
- per-deck `due` = `capped.length` (studyable-now for this deck: kept new + kept
  review + all in-progress).
- `queue` = the capped cards across all decks (`capped.map((c) => c.card)`), in
  deck order (the All-decks path still `shuffle`s this).
- `totalNew` = Σ per-deck `newCount`; `totalDue` = Σ per-deck `due`;
  `totalReview` = `totalDue − totalNew`. (Invariant `totalDue = totalNew +
  totalReview` preserved, so `DeckListPage`'s chips stay consistent.)

`DeckOverview.total` (all cards in deck) and the `isNew` helper are unchanged.
`DeckListPage`/`DeckDetailPage` JSX is untouched — the numbers are just capped.
(`DeckDetailPage` computes its own `newCount` from cards and is unaffected.)

## `ReviewPage`

- **Build.** Single deck: `day = localDay(now)`,
  `stat = await storage.getDailyStat(deckId, day)`, build `caps` from
  `settings.newPerDay/maxReviews − stat`, pass to `buildSessionCards`. All decks:
  `loadDueOverview` already returns the capped `queue`; keep the existing
  `settingsById` + `shuffle` mapping.
- **Grade.** After the existing `storage.updateCard(...)`, bump the counter from
  the **pre-grade** state, keyed by the **card's own deck** (correct for
  All-decks) and today's `day`:

  ```ts
  const preState = current.card.scheduling.state
  // ... await storage.updateCard(current.card.id, { scheduling: nexts[g] })
  if (preState === 0) await storage.bumpDailyStat(current.card.deckId, day, 'newIntroduced')
  else if (preState === 2) await storage.bumpDailyStat(current.card.deckId, day, 'reviewsDone')
  ```

  `day = localDay(Date.now())` at grade time. The in-flight double-grade guard
  from the #3a follow-up already prevents a single answer from bumping twice.

## Out of scope (later slices)

- Full per-grade review log and FSRS weight training — **#5**.
- Leech detection/action and burying — **#3c**.
- A configurable rollover hour (see [D2]).
- Syncing/backing up daily counters across devices.

## Verification

1. `npm run typecheck` — green (seam + `buildSessionCards` signature propagate).
2. `npm test` — green on the fake scheduler:
   - `day.test.ts`: `localDay` format; midnight boundary (adjacent strings).
   - `session.test.ts`: cap partition — new capped to `newSlots`, reviews capped
     to `reviewSlots`, in-progress always kept, zero/negative slots keep no
     new/review but still serve in-progress, ordering unchanged from #3a.
   - `dueOverview.test.ts`: per-deck and aggregate counts reflect caps;
     `totalDue = totalNew + totalReview`; capped `queue`.
   - `DexieStorage` test: `getDailyStat` default zeros; `bumpDailyStat` upsert +
     increment; `deleteDeck` removes the deck's stats.
   - `migration.test.ts`: v8 additive — existing decks/cards intact, empty
     `dailyStats` after upgrade.
   - `ReviewPage` browser test: a `newPerDay = 1` deck serves one new card then
     completes; grading bumps `newIntroduced`; reopening the same day grants no
     more new cards.
3. `cd src-tauri && cargo test` — unchanged, still green (no Rust edit).
4. Manual (`npm run app:dev`): set a deck to `newPerDay 2`, study — exactly two
   new cards appear; the Today screen shows the capped counts.

## Accepted trade-offs

- **Rollover at local midnight, not 4am** (see [D2]).
- **Counters are local-only** — not synced or backed up; a restore/other device
  starts the day's allowance fresh.
- **Counters, not a review log** — no per-grade history until #5; "reviews done"
  is an answer count, sufficient for the cap.
- Review-time scheduling stays **async + native-only** (inherited from #2).
</content>
</invoke>
