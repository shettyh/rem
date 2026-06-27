# Card Editor — Full-Featured Screen with Image/GIF Assets

**Date:** 2026-06-26
**Status:** Approved design, pending implementation plan
**Scope:** Feature #1 from `features.md` ("Fully featured edit/add card screen instead of pop up").

## Overview

Replace the in-page `CardEditorModal` with a dedicated full-screen editor route that
carries a persistent, GitHub-style markdown toolbar and supports local image/GIF
content. Images are stored as content-addressed blobs behind the existing `Storage`
port, referenced from card markdown as `asset:<hash>`, rendered in both the editor and
review views, and synced across machines as real binary files in the git repo.

## Goals

- A real editor *screen* (route), not an overlay — back-button, deep-link, no escape-key plumbing.
- A persistent toolbar exposing the markdown tools the editor already supports, plus an image button.
- Insert local images/GIFs via toolbar pick, paste, or drag-and-drop. Works offline.
- Images render correctly in the editor and during review.
- Images sync across machines without bloating the git repo.

## Non-Goals (explicitly out of scope)

- Tags/labels (feature #4) — separate spec; grafts onto this editor later.
- Remote-URL images, video, audio.
- Image editing (resize, crop, annotate).
- Changing the scheduling, deck, or review model.

## Design Decisions (resolved)

| Decision | Choice | Why |
|----------|--------|-----|
| Image source | Local files only (paste/drag/pick) | Confirmed use case; offline-first. |
| Storage strategy | Content-hashed blobs behind the `Storage` port (`asset:<hash>` refs) | Fits the single-seam architecture; testable under fake-indexeddb; dedupes; clean markdown. |
| Form factor | Dedicated route | Real history/back-button/deep-link; removes overlay plumbing. |
| Asset sync | Included now (end-to-end) | No cross-machine broken-image gap. |
| Bubble menu | Dropped in favor of the toolbar | One control surface; avoids duplicate command paths. |
| Orphan GC | Sweep on card save + delete | Keeps repo lean; full scan is fine at current scale. |
| Heading control | Three buttons (H1/H2/H3) | Simplest; matches `heading: { levels: [1,2,3] }`. |

## Architecture

### Reference convention

Images are referenced in card markdown as `![alt](asset:<hash>)`, where `<hash>` is the
SHA-256 of the image bytes (computed with Web Crypto `crypto.subtle.digest`, available in
the renderer and under vitest/node). The `asset:` scheme is the seam between the card's
markdown and where the bytes live — the JSON snapshot never contains a raw blob, and
identical images dedupe to one record.

### Domain model (`src/domain/models.ts`)

```ts
export interface Asset {
  hash: string        // SHA-256 of bytes; primary key
  mime: string        // e.g. image/png, image/gif
  bytes: Uint8Array   // raw image bytes
  createdAt: number
}
```

### Storage port (`src/data/Storage.ts`)

New methods:

- `putAsset(bytes: Uint8Array, mime: string): Promise<Asset>` — hash, dedupe, store, return.
- `getAsset(hash: string): Promise<Asset | undefined>`.
- `sweepOrphanAssets(): Promise<void>` — delete assets referenced by no card. Full scan of
  all cards for `asset:<hash>` refs; acceptable at current scale.

`exportSnapshot()` and `applyMerge()` are extended to carry referenced asset blobs (see Sync).

### Dexie (`src/data/dexie/`)

New `assets` table keyed by `hash`, storing `bytes` as a `Blob`. Added via a schema version
bump with a migration, alongside the existing tables. `DexieStorage` implements the new
`Storage` methods.

### Editor screen & routing

- New routes in `src/app/routes.tsx`:
  - `/decks/:deckId/cards/new` → `CardEditorPage` (create)
  - `/decks/:deckId/cards/:cardId/edit` → `CardEditorPage` (edit)
- `CardEditorPage` reuses today's load/save/delete logic, rendered full-page with
  `PageHeader` (Save / Cancel / Delete-when-editing). Save and Cancel `navigate` back to
  the deck page.
- `DeckDetailPage`: "+ Add card" and card rows `navigate()` to these routes instead of
  setting modal state.
- `CardEditorModal.tsx` is deleted, along with the `modal-*` CSS orphaned by its removal.

### Toolbar (`RichMarkdownEditor`)

A persistent toolbar above the editor content. Buttons map to existing TipTap commands:
**H1, H2, H3, bold, italic, inline code, code block, bullet list, numbered list,
blockquote, link, image**. The link button keeps the current `window.prompt` flow. The
bubble menu is removed.

### Asset ingestion

A single helper `insertImage(file)`:
1. read bytes,
2. `storage.putAsset(bytes, file.type)`,
3. insert an image node with `src: "asset:<hash>"`.

Wired from three entry points through TipTap `editorProps`:
- toolbar image button → hidden `<input type="file" accept="image/*">`,
- `handlePaste` → clipboard image items,
- `handleDrop` → dragged image files.

`RichMarkdownEditor` stays storage-agnostic and testable: it takes
`ingestImage?: (file: File) => Promise<{ hash: string; mime: string }>`. The image button is
hidden when the prop is absent. `CardEditorPage` supplies it from `useStorage`.

### Asset rendering

A shared hook `useAssetUrl(hash)` loads bytes from `Storage`, creates an object URL, and
revokes it on unmount. Two consumers:

- **Editor:** a custom TipTap Image node-view resolves `asset:<hash>` → object URL for display.
- **Review/preview (`MarkdownView`, react-markdown):** override the `img` renderer so
  `asset:<hash>` srcs resolve via the same hook.

### Sync (binary transport)

Assets are immutable and content-addressed, so reconciliation is simpler than cards:
**union by hash, then prune to hashes referenced by merged cards** — no last-writer-wins.

- `RepoSnapshot` gains `assets: AssetBlob[]` (`{ hash, mime, bytes }`). `exportSnapshot`
  gathers blobs referenced by local cards; `applyMerge` persists newly-arrived blobs locally
  so pulled cards render.
- On-disk layout: `assets/<hash>.<ext>` (ext derived from mime; mime recovered from ext on
  read — no separate manifest).
- `GitBridge` gains `readAssets(dir)` and `writeAssets(dir, assets)`. Two new Rust commands
  read/write `assets/` as **raw binary**; base64 is only the transient IPC encoding, so git
  stores true binary blobs. `writeAssets` clears `assets/` first (delete-absent semantics,
  matching `git_write_files`). `FakeGitBridge` implements both in-memory.
- `GitSyncService.sync()` reads/writes assets alongside the text files; the existing
  `git add -A` commits them.

## Testing Strategy

- **Unit (vitest + fake-indexeddb):** SHA-256 hashing + dedupe; `putAsset`/`getAsset`/
  `sweepOrphanAssets`; snapshot asset round-trip; asset merge (union + prune to referenced).
- **Browser (vitest-browser):** toolbar commands apply formatting; paste and drop ingest an
  image; image renders in the editor and in review.
- **Rust:** `git_read_assets`/`git_write_assets` binary round-trip and delete-absent.
- **Integration:** `FakeGitBridge` two-clone test proving an image syncs from clone A to clone B.

## Risks & Notes

- **Full-scan orphan sweep** is O(cards). Acceptable now; revisit if decks grow large.
- **Binary over IPC** uses base64 transiently — ~33% transport overhead during sync only;
  on-disk and in-git the blobs are raw. Acceptable.
- **`git_read_files` must keep ignoring `assets/`** (it reads text); assets travel only
  through the new asset commands, so the existing UTF-8 read path is untouched.
- Removing the bubble menu changes existing browser-test expectations that assert on it;
  those tests move to the toolbar.
