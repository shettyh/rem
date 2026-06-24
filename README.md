# rem

A clean, local-first spaced-repetition flashcard **desktop app** — markdown cards (text + code),
deck organisation, FSRS/SM-2 scheduling, and git-backed sync across machines. Think AnkiDroid,
less clunky.

> **Native only.** rem is a [Tauri](https://tauri.app) desktop app. It syncs by shelling out to
> your system `git`, which a browser can't do, so the web build is unsupported — opening the dev
> URL in a browser just shows a "desktop app" notice. Run it with `npm run app:dev`.

## Stack

React + TypeScript + Vite, packaged as a native app with **Tauri v2** (Rust). Local data in
**Dexie** (IndexedDB); **TipTap** WYSIWYG-markdown editor; **react-markdown** for rendering;
**ts-fsrs** / SM-2 for scheduling.

## Run & build

```bash
npm install
npm run app:dev     # run the native app — this is how you use rem
npm run app:build   # build native installers for your platform
npm test            # unit + real-browser UI tests (Vitest + Playwright)
npm run typecheck   # tsc --noEmit
```

`npm run dev` and the printed `http://localhost:5173` are **only the internal webview source** that
`npm run app:dev` loads — not a browser app. Open the desktop window, not the URL.

## Architecture

Dependencies point inward toward stable interfaces:

- **`Scheduler`** (`src/domain/scheduler`) — the scheduling algorithm; FSRS or SM-2, chosen per
  deck. Pure and unit-tested.
- **`Storage`** (`src/data/Storage.ts`) — the persistence port; Dexie (IndexedDB) today, with a
  git-sync backend behind the same seam.

```
src/
  domain/      models + scheduler (Scheduler interface, FSRS + SM-2)
  data/        Storage interface + Dexie impl + git sync + React context
  features/    decks · cards (TipTap editor + view) · review · settings
  app/         entry, router
  ui/          app shell (sidebar + content) + design tokens / styles
src-tauri/     Rust: window chrome (overlay titlebar, sidebar vibrancy) + git bridge
```

## Sync

Decks and cards sync through a GitHub (or any git) remote via the system `git` and your existing
credentials — no token is stored. Per-record last-writer-wins with tombstones, behind the `Storage`
seam. Configure the remote in **Settings → Sync**.

## Scheduling

New decks default to **FSRS** (`ts-fsrs`); existing decks stay on **SM-2**. Both sit behind the
`Scheduler` interface, so swapping or adding an algorithm is a one-file change.
