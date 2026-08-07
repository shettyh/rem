# rem

A clean, local-first spaced-repetition flashcard **desktop app** — markdown cards (text + code),
deck organisation, FSRS scheduling, review statistics, and git-backed sync across machines. Think
AnkiDroid, less clunky.

> **Native only.** rem is a [Tauri](https://tauri.app) desktop app. It syncs by shelling out to
> your system `git`, which a browser can't do, so the web build is unsupported — opening the dev
> URL in a browser just shows a "desktop app" notice. Run it with `npm run app:dev`.

## Stack

React + TypeScript + Vite, packaged as a native app with **Tauri v2** (Rust). Local data in
**Dexie** (IndexedDB); **TipTap** WYSIWYG-markdown editor; **react-markdown** for rendering;
**ts-fsrs** for scheduling.

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

## Install

**macOS / Linux** — one-liner (no Apple Developer warning; `curl` downloads skip
macOS quarantine):

```bash
curl -fsSL https://raw.githubusercontent.com/shettyh/rem/main/install.sh | sh
```

macOS copies `rem.app` to `/Applications`; Linux installs the AppImage to
`~/.local/bin/rem` (needs FUSE; on Debian/Ubuntu `sudo apt install libfuse2`).

**Manual download** — grab a build from the
[Releases page](https://github.com/shettyh/rem/releases). Because rem isn't
notarized (no paid Apple Developer ID), a **browser-downloaded** DMG triggers a
Gatekeeper warning. Either right-click the app → **Open** → **Open**, or clear
quarantine after copying it to Applications:

```bash
xattr -dr com.apple.quarantine /Applications/rem.app
```

**Windows** — download the `.msi`/`.exe` from the Releases page.

## Cutting a release

The [`release.yml`](.github/workflows/release.yml) workflow builds installers for
macOS (Apple Silicon + Intel), Linux, and Windows on every `v*` tag.

1. Bump the version to match in all three files: `package.json`,
   `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
2. Commit, then tag and push:
   ```bash
   git tag v0.1.1 && git push origin v0.1.1
   ```
3. The workflow opens a **draft** GitHub Release with the installers attached.
4. **Publish the draft.** Required — the curl installer reads `releases/latest`,
   which ignores drafts and prereleases, so it only finds a published release.

## Architecture

Dependencies point inward toward stable interfaces:

- **`Scheduler`** (`src/domain/scheduler`) — the scheduling algorithm; FSRS today, behind an
  interface so another algorithm stays a one-file addition. Pure and unit-tested.
- **`Storage`** (`src/data/Storage.ts`) — the persistence port; Dexie (IndexedDB) today, with a
  git-sync backend behind the same seam.

```
src/
  domain/      models + scheduler (Scheduler interface, FSRS + SM-2)
  data/        Storage interface + Dexie impl + git sync + React context
  features/    decks · cards (TipTap editor + view) · review · settings
  app/         entry, router
  ui/          app shell (sidebar + content) + design tokens / styles
src-tauri/     Rust: window chrome (overlay titlebar) + git bridge
```

## Sync

Decks and cards sync through a GitHub (or any git) remote via the system `git` and your existing
credentials — no token is stored. Per-record last-writer-wins with tombstones, behind the `Storage`
seam. Configure the remote in **Settings → Sync**.

## Scheduling

All cards are scheduled with **FSRS** (`fsrs-rs` in Rust). It sits behind the `Scheduler` interface,
so swapping or adding an algorithm remains isolated. FSRS-effective grades are recorded locally and
synced with the deck; **Deck options → FSRS parameters** can optimize or reset per-deck weights once
a card has been reviewed on a later day.
