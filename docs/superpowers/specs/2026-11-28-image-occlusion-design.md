# Image Occlusion — Markdown-Native Design

_Date: 2026-11-28 · Branch: `feat/image-occlusion`_

## Goal

Add first-class, zero-add-on **Image Occlusion** across Desktop, iOS, `.apkg` import, backup, and Cloud sync — with **zero SQLite schema migration**, **zero sync-protocol changes**, and **zero external canvas/annotation dependencies**.

## Storage Format (`rem-occlusion`)

An Image Occlusion card stores its diagram reference and normalized `0..1` mask geometry inside a fenced `rem-occlusion` block in `card.front`, optionally surrounded by normal Markdown header/footer text:

````markdown
TCP Header

```rem-occlusion
{"v":1,"image":"asset:<64-hex-sha256>","mode":"hide-all","alt":"TCP Header","masks":[{"id":1,"type":"rect","x":0.05,"y":0.1,"w":0.4,"h":0.15},{"id":1,"type":"ellipse","x":0.55,"y":0.1,"w":0.4,"h":0.15},{"id":2,"type":"rect","x":0.05,"y":0.35,"w":0.9,"h":0.15}]}
```
````

### Key invariants
- **Schema & sync reuse:** Reuses SQLite schema v6's `sibling_group_id` and `cloze_index` columns, `DeckSettings.bury_related` (including atomic undo), R2 content-addressed asset sync, orphan asset sweeping (`asset:<64-hex>`), and JSON backup round-trip without any schema bump.
- **Sibling grouping:** Each distinct positive integer `mask.id` (`1, 2, ...`) produces one independently scheduled sibling `Card` with `cloze_index = Some(id)`. Multiple shapes sharing an `id` reveal together as one card.
- **Modes:**
  - `hide-all` (*Hide All, Guess One*, default): covers all masks on the front while highlighting the active mask; after reveal, unveils the active mask with an outline and offers a quiet **Show all masks** toggle.
  - `hide-one` (*Hide One, Guess One*): covers only the active mask on the front so surrounding labels remain visible for context.
- **Validation & bounds (enforced identically in Rust, TypeScript, and Swift):**
  - `v == 1`, `image` strictly matches `asset:[0-9a-f]{64}` (no remote URLs or script schemes).
  - Normalized coordinates clamped to `[0.0, 1.0]`, masks below `0.002` width/height or `id < 1` discarded, capped at `200` masks per block.

## Surface Implementation

1. **`rem-core` (`occlusion.rs`, `store.rs`, `apkg.rs`):**
   - `parse_occlusion_blocks`, `extract_occlusion_indices`, `serialize_occlusion_block`, `strip_occlusion_markup`, `render_occlusion_tui_summary`.
   - `create_card_or_cloze` and `update_cloze_note` extract combined sibling indices from text cloze (`{{cN::...}}`) and `rem-occlusion` blocks, propagating edits across siblings, deleting unreviewed removed mask cards, suspending reviewed removed mask cards, and creating cards for newly added mask groups.
   - `.apkg` importer detects Anki 23.10+ built-in Image Occlusion notes (`{{cN::image-occlusion:rect|ellipse|poly:...}}`), preserves `oi=1` (`hide-all`) vs `oi=0` (`hide-one`), maps the `Image`, `Header`, and `Back Extra` fields into `rem-occlusion` cards with shared `sibling_group_id`, media assets, and FSRS-7 `revlog` replay.
2. **Desktop (custom React + SVG):**
   - `OcclusionView.tsx` renders resolution-independent SVG overlays (`vectorEffect="non-scaling-stroke"`) styled with Quiet Paper CSS tokens, plus post-reveal **Show all masks** toggle.
   - `OcclusionEditor.tsx` embeds directly in `CardEditorPage` with Rectangle (`R`), Ellipse (`E`), Select/Move/8-Handle Resize (`V`), Pan (`H`), Zoom (`50%–300%`), Shift-click multi-select Grouping/Ungrouping, keyboard nudging, and Undo/Redo.
   - Card lists (`DeckDetailPage`) and `SearchModal` strip raw JSON into readable titles and show `IO N` badges.
3. **iOS (native SwiftUI):**
   - `ImageOcclusionView.swift` implements `ImageOcclusionParser`, `AssetOcclusionImageView` (aspect-fitted native overlay + post-reveal **Show all masks** toggle), spoiler-safe `renderCompositeImage` for `ImageLightboxView` pinch-to-zoom, and `ImageOcclusionEditorSheet` for creating/editing masks on iPhone.
4. **Terminal TUI (`rem study`):**
   - Displays a concise `[Image Occlusion: ... · mask #N of M]` placeholder rather than raw JSON, preserving header text, mask labels on reveal, and Back/Extra notes.
