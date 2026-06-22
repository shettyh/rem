# rem — Progress & Roadmap

_Last updated: 2026-06-21_

## Where we are (MVP shipped)

The smallest genuinely-useful loop is built and tested:

- **Decks** — create, list with live due counts.
- **Cards** — markdown front/back with code highlighting; create / edit / delete.
- **Review** — due queue, flip, 4-button grading, keyboard shortcuts.
- **Local-first** — all data in IndexedDB (Dexie), behind a `Storage` interface.
- **Scheduling** — SM-2 behind a `Scheduler` interface.

Verified: scheduler (9 tests), storage (7 tests), review cycle (1 integration test), build + typecheck green.
Plus a real-browser UI test harness (Vitest browser mode + Playwright Chromium) — **34 tests total**.

**Two extension seams already exist** — `Scheduler` and `Storage`. Most of what follows slots behind
an interface rather than rewriting the app. New capability should keep that discipline.

---

## The key insight that reframes half these ideas: embeddings ≠ generation

When we say "local embedded model," there are two very different things:

| | Local **embedding** model | Local **generative** LLM |
|---|---|---|
| Size | ~20–90 MB (e.g. all-MiniLM) | 1–4+ GB |
| Runtime | transformers.js, WASM (WebGPU optional) | WebLLM, **needs WebGPU** |
| Quality at small size | excellent (that's what they're for) | mediocre / inconsistent |
| Good for | dedup, "related cards", semantic search, smart interleaving | writing/summarizing cards from source text |
| **Feasible locally today?** | **Yes, easily** | Feasible but heavy — better as opt-in / cloud first |

So: **embedding-based features are very feasible locally and unlock things Anki doesn't have.**
Generative features (writing cards for you) are real but should start cloud/opt-in for quality, with
local WebLLM as a later privacy option.

---

## Answers to the five questions

**1. Beautiful UI (currently a toy).** Agreed — the logic is solid but the surface is plain CSS.
A dedicated design pass: a real type scale, refined spacing/color tokens, a polished study screen
(card-flip motion, calm grading bar), nice empty states, and subtle micro-interactions. This pairs
naturally with #4 — the editor is the most-used surface, so redesign it first.

**2. Novel vs Anki.** Best candidates, most of which lean on a *local embedding* model:
- **Duplicate guard** — on add, warn "you already have a similar card" (embedding similarity).
- **Related cards** — surface semantically-linked cards during review (knowledge graph).
- **Smart interleaving** — compose sessions across decks by topic, not just due date.
- **Run-the-code cards** — execute a snippet inline for programming cards.
- **Auto-cloze** — select text → generate a cloze deletion.
- **Lapse notes** — capture "why I missed it" on Again; resurface on next review.
- **Calendar-aware load** — smooth due spikes so a busy day doesn't dump 200 cards.

**3. Local model for a better study plan — _is it too far-fetched?_**
Honest pushback: for *scheduling*, an LLM is the wrong tool. The right answer is **FSRS** (the
algorithm modern Anki uses) — a tiny, memory-model-based scheduler with proven JS implementations.
That *is* "a model for better study plans," just the correct kind: small, deterministic, runs
instantly, no GB download. It drops straight behind our existing `Scheduler` interface. Recommended.

**4. Single edit window with inline markdown (not two panes).**
Strongly agree — the two-pane editor feels like a dev tool. We want a **WYSIWYG-markdown editor**:
you type, it renders inline (Obsidian/Typora style), and markdown stays the source of truth.
Candidates: **TipTap** or **Milkdown** (both ProseMirror-based, markdown-native, with code-block
support). This replaces `MarkdownEditor` + the preview pane and is the single biggest UX upgrade.

**5. Upload data → local model drafts smart cards → user reviews & saves.**
Not far-fetched, but scope it. Introduce a `CardGenerator` interface (same pattern as `Scheduler`/
`Storage`). Flow: import text/file → generator proposes draft cards → user edits/accepts/rejects →
save. Start with a **bring-your-own-key cloud model (Claude)** for quality, add an optional
**in-browser WebLLM** path later for fully-local/private generation. The review-before-save gate is
the important part.

---

## Roadmap (proposed order)

**Near-term (UX & polish)**
1. ✅ **Single WYSIWYG-markdown editor** (#4) — **shipped**: TipTap v3 inline editor with
   markdown input rules, syntax-highlighted code blocks, and a selection bubble menu;
   replaced the two-pane CodeMirror editor. Markdown remains the stored source of truth.
2. ✅ **Real-browser UI test harness** — **shipped**: Vitest browser mode + Playwright Chromium
   renders components and full pages with real CSS and screenshots (split `unit`/`browser` projects;
   `npm test` runs both). A `StorageProvider` test seam + `freshStorage`/`renderRoute`/`shoot` helpers
   drive an 11-screen sweep. Produced a screenshot-grounded issues list
   (`docs/superpowers/specs/2026-06-20-ui-issues.md`) as the input for the redesign.
3. ✅ **UI redesign pass** (#1) — **shipped**: CSS token system, light/dark themes (no-FOUC toggle), true 3D study-card flip, and per-surface fixes (deck list, detail, editor, review end states).
4. ✅ **Export / import backup** — **shipped**: a reusable Settings page (`/settings`, header ⚙)
   with deck-scoped JSON export (multi-select + select-all) and replace-by-name import behind a
   confirm-before-replace warning. Full-fidelity round-trip (scheduling preserved); all behind the
   existing `Storage` seam (pure `backup.ts` + atomic `Storage.importDecks`).

**Mid-term (smarter, still local)**
4. **FSRS scheduler** (#3) behind the existing `Scheduler` interface.
5. **Embedding-powered features** (#2) — duplicate guard + related cards + semantic search, via a
   local embedding model (transformers.js).

**Longer-term (generative, opt-in)**
6. **AI card generation from uploads** (#5) — `CardGenerator` interface; BYO-key cloud first,
   local WebLLM later; review-before-save.
7. **Remaining novel ideas** (#2) — run-the-code cards, auto-cloze, lapse notes, calendar-aware load.

Each item gets its own brainstorm → plan → TDD cycle, like the MVP.
