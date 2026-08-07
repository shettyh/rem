# Stats screen — implementation plan

_Date: 2026-08-07_

**Goal:** Add a filterable, review-log-backed Stats screen with faithful activity and recall metrics.

**Architecture:** A pure aggregation module produces one view model from decks/logs/time/filter.
`StatsPage` only loads through `Storage`, holds controls, and renders dependency-free CSS charts.

## 1. Pure aggregation

- Add failing tests for range buckets, recall and grade distribution, active days, current streak,
  deck filter, and deck breakdown ordering.
- Implement `src/features/stats/stats.ts` with local-calendar helpers and explicit summary types.
- Verify focused unit tests.

## 2. Stats page

- Add failing browser tests for empty history and seeded review metrics.
- Implement `StatsPage` with deck selector, 7/30/90 controls, summary cards, activity bars, grade bars,
  deck breakdown, and history-scope note.
- Add only Stats-specific CSS using existing tokens and responsive conventions.
- Verify focused browser tests in light and dark themes where relevant.

## 3. Navigation and visual sweep

- Register `/stats` and add a Stats sidebar link with active state.
- Add a browser navigation assertion.
- Add a seeded Stats scenario to `screens.browser.test.tsx` for light/dark screenshots.

## 4. Close out

- Run `npm test` and `npm run build`.
- Run Rust fmt, Clippy, and tests to preserve repository gates.
- Update roadmap/continuation notes and README navigation summary.
- Review the diff and keep `features.md` untouched/untracked.
