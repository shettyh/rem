# Stats screen — design

_Date: 2026-08-07_

## Context

Dexie v11 now stores immutable, synced `ReviewLog` records for grades that update FSRS memory.
`features.md` asks for a Stats screen, but does not define its metrics or visual design. This slice
uses only facts the current data can answer faithfully.

## Goal

Add a global `/stats` screen that summarizes recorded FSRS review activity, supports deck and time
filters, and remains useful without a charting dependency.

Success =
- Stats is a first-class sidebar destination.
- The screen supports All decks or one deck and 7 / 30 / 90 day windows.
- Summary cards show FSRS reviews, recall rate, current streak, and active days.
- Daily activity, grade distribution, and per-deck breakdown agree with the same pure aggregation.
- Empty and partial-history states explain that logs begin with the review-history migration and
  exclude fixed learning-step clicks.

## Decisions

### Metrics in the first slice

For the selected deck scope and date window:

- **FSRS reviews** — number of `ReviewLog` events.
- **Recall rate** — `(Hard + Good + Easy) / all grades`; Again is a miss.
- **Active days** — distinct local calendar days containing at least one event.
- **Daily activity** — one zero-filled bucket per local day in the selected window.
- **Grade distribution** — count and percentage for Again / Hard / Good / Easy.
- **Deck breakdown** — review count and recall rate per included deck, ordered by activity.

**Current streak** uses all history in the selected deck scope rather than truncating to the visual
range. It counts consecutive local calendar days ending today; if today has no activity but yesterday
does, the streak remains active because today has not been missed yet. Otherwise it is zero.

### Range boundaries are local-calendar based

Ranges are 7, 30, or 90 local calendar days including today. The first bucket begins at local
midnight; events after `now` are excluded. Day stepping uses `Date.setDate()` so DST transitions do
not create duplicate/missing labels.

### Pure aggregation, thin page

`src/features/stats/stats.ts` owns all calculations:

```ts
type StatsRange = 7 | 30 | 90
buildStats(logs, decks, now, range, deckId | null): StatsSummary
```

The function is deterministic and storage-free. `StatsPage` loads decks plus each deck's logs
through the existing `Storage` seam, owns filter state, and renders the summary.

### No chart dependency

Daily activity uses a CSS flex bar chart with accessible text. Grade distribution uses horizontal
CSS bars. This avoids adding a large dependency for two simple visualizations and keeps theme tokens
consistent.

### Navigation and empty states

Add **Stats** next to Today in the sidebar and register `/stats` in the router.

When no logs exist in the selected scope, show `No review history yet` while keeping filters visible.
When history exists outside the selected range, show a range-specific no-activity message. Always
show a note that stats contain only recorded FSRS-effective reviews and do not reconstruct earlier
history.

## Out of scope

- Forecasted workload/due counts.
- Card-state, stability, difficulty, or retrievability distributions.
- Time-spent analytics (not recorded).
- Learning-step click counts.
- Longest streak, heatmap calendar, goals, achievements, or CSV export.
- Persisting filter choices.

## Verification

1. Unit tests cover local-day boundaries, zero-filled ranges, recall/grade counts, current streak,
   deck filtering, and per-deck ordering.
2. Browser tests cover empty state, seeded metrics, range/deck controls, accessible chart copy, and
   sidebar navigation.
3. Add Stats to the light/dark real-browser screenshot sweep.
4. `npm test`, `npm run build`, Rust fmt/Clippy/tests (unchanged Rust).
