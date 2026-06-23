# rem

A clean, local-first spaced-repetition flashcard webapp — markdown cards (text + code),
deck organization, and SM-2 scheduling. Think AnkiDroid, less clunky.

## Stack

React + TypeScript + Vite · Dexie (IndexedDB) · CodeMirror (editor) · react-markdown (render).
All data lives in the browser; no backend or account.

## Commands

```bash
npm run dev        # start the dev server (http://localhost:5173)
npm test           # run the unit + integration tests (Vitest)
npm run build      # typecheck + production build
```

## Architecture

Dependencies point inward toward two stable interfaces:

- **`Scheduler`** (`src/domain/scheduler`) — the scheduling algorithm. SM-2 today; swap the
  one line in `index.ts` to change it. Pure and fully unit-tested.
- **`Storage`** (`src/data/Storage.ts`) — the persistence port. Backed by IndexedDB via
  `DexieStorage`; a sync backend can implement the same interface later.

```
src/
  domain/      models + scheduler (Scheduler interface, SM2Scheduler)
  data/        Storage interface + Dexie implementation + React context
  features/    decks · cards (markdown editor + view) · review (study session)
  app/         entry, router
  ui/          layout + global styles
```

## Scheduling

Review grades map to SM-2 quality scores: Again→0, Hard→3, Good→4, Easy→5. New and lapsed
cards repeat in a day; successful cards grow 1 → 6 → ×ease days. Ease never drops below 1.3.

## Not yet built (next iterations)

Images in cards · nested deck namespacing · stats dashboard.
