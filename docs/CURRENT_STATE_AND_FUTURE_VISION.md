# rem — Current State & Future Vision

_Last reviewed: 2026-08-09 · based on `v0.8.0` plus the agent-first learning-loop work_

This document is the canonical product snapshot for future planning. Dated files under
`docs/superpowers/` record the decisions and implementation history behind individual slices;
`docs/ROADMAP.md` is an earlier roadmap and no longer accurately describes the product.

## Product vision

rem should turn useful work and deliberate study into durable human memory. It combines modern FSRS
scheduling with a calm local-first interface and a safe rendezvous between agents and the
human-owned study plan:

```text
agent does useful work
        ↓
proposes a few source-grounded recall prompts
        ↓
human tries, inspects, edits, accepts, or rejects each draft
        ↓
accepted cards enter normal FSRS scheduling
        ↓
human studies in the desktop app or terminal
```

The product should make five things especially easy:

1. **Capture** a durable card from the app or terminal without interrupting the user's work.
2. **Turn agent work into learning opportunities** without silently scheduling generated content.
3. **Review** the right cards in a calm, fast, keyboard-friendly desktop or terminal session.
4. **Understand and tune** the study plan without requiring scheduling expertise.
5. **Own and move the data** without depending on an account, hosted service, or model provider.

### Product principles

- **The human is the learner and approval authority.** Agents may notice and formulate learning
  opportunities, but generated content remains a draft until a human accepts it.
- **Local-first and user-owned.** SQLite is primary. Backup and sync remain inspectable and portable.
- **Correct model for the job.** FSRS schedules reviews; an LLM does not.
- **Provider-neutral agent integration.** A versioned CLI is the public seam. rem does not require or
  embed an agent runtime or model provider.
- **One review implementation.** Desktop and terminal adapters use the same Rust `StudySession` so
  scheduling, caps, learning steps, leeches, and concurrency behavior cannot drift.
- **Simple before clever.** Prefer a dependable proposal → approval → study loop over speculative
  automation.
- **Stable seams.** Scheduling, persistence, sync transport, and agent-facing operations stay behind
  small intent-level interfaces.

## Current state

rem is a local-first Tauri v2 desktop application with a companion `rem` CLI. React, TypeScript, and
Vite render the desktop webview; Rust owns native persistence, scheduling, study sessions, and CLI
commands. The browser build is an internal webview/test adapter, not a supported browser product.

### Agent-first capture and draft approval

- `rem deck list`, `rem card add`, `rem draft add`, and `rem draft list` work while the desktop app is
  closed and support stable versioned JSON for agents and scripts.
- Card and draft batches validate atomically, resolve deck IDs or exact names, normalize tags,
  support dry runs, and return retry-safe exact-duplicate outcomes.
- Agent proposals are stored as a separate `CardDraft` domain value with rationale, opaque source
  locators, producer, timestamps, and optimistic revision.
- Pending drafts have no scheduling state, due date, review history, or suspension state. They do not
  affect counts, stats, backup, Git sync, or the synchronized collection revision.
- The desktop **Drafts** inbox is front-first: the human tries the question, reveals the proposed
  answer and provenance, edits content/tags/deck, then accepts or rejects it.
- Acceptance atomically creates a normal due card through the collection rules and removes the
  draft. A card created after proposal is recognized as an existing exact duplicate.
- Draft acceptance/rejection and review grading use optimistic local revisions, preventing two open
  views from committing stale decisions.
- The bundled `rem-card-capture` agent skill proposes at most three source-grounded drafts, treats
  zero as valid, dry-runs first, and never claims a pending draft is scheduled or synced.
- Trusted automation can still use direct `rem card add` when bypassing human approval is explicit.

### Card editing and assets

- Full-screen add/edit experience using a TipTap WYSIWYG-Markdown editor; Markdown remains the
  stored source of truth.
- Formatting toolbar for headings, emphasis, lists, links, inline code, and fenced code blocks.
- Syntax-highlighted code when editing and reviewing.
- Image and GIF ingestion through file picker, paste, or drag-and-drop.
- Content-addressed binary assets render in cards and travel through backup and Git sync.
- User-defined tags, tag chips, and per-deck tag filtering.
- Create, edit, delete, suspend, and restore cards.

### Decks and study

- Flat deck organization with colors, live due counts, and cross-deck study.
- FSRS-6 calculation and optimization live in `rem-core`; SM-2 has been removed.
- A shared Rust `StudySession` owns due selection, daily caps, learning/relearning steps, FSRS
  choices, custom study, leeches, review persistence, counters, requeueing, and stale-card conflicts.
- The desktop review page is a rendering/input adapter over process-local Tauri study sessions.
- `rem study` reviews all decks; `rem study --deck <id-or-exact-name>` reviews one deck through the
  same `StudySession` implementation.
- The interactive TUI supports Space/Enter reveal, 1–4 grading, arrow/j/k scrolling, resize, clean
  `q` exit, interval previews, progress, completion counts, leech notices, and stale-card messages.
- Terminal Markdown renders headings, lists, emphasis, links, inline/fenced code, and explicit
  image/GIF placeholders containing asset hashes.
- Custom desktop study modes include study ahead, additional new cards, forgotten cards, and
  non-rescheduling new-card preview.
- Per-deck controls cover desired retention, learning/relearning steps, new/review limits, insertion
  order, minimum/maximum intervals, leech behavior, and personalized FSRS weights.

### Insight and history

- Review history records FSRS-effective grades.
- Stats dashboard supports global or per-deck scope, 7/30/90-day ranges, review activity, recall
  rate, streak, active days, grade distribution, and deck breakdown.
- Daily-cap counters are local to each machine; review history is included in backup and sync.

### Data, backup, and sync

- Native desktop and CLI operations use SQLite through `rem-core`; the TypeScript `Storage`
  interface keeps React independent of native commands, and Dexie remains a browser-test adapter.
- SQLite schema v2 contains local-only drafts and per-card optimistic `local_revision` values.
- Deck-scoped JSON export/import preserves scheduling and review history, with confirmation before a
  same-named deck is replaced.
- Optional Git-backed sync uses the user's system `git` and existing credentials; rem stores no
  access token.
- Per-record last-writer-wins merge, tombstones, and synchronized content-addressed assets are
  implemented.
- Unapproved drafts and local revisions are intentionally excluded from backup and Git sync.
- Manual sync from Settings and automatic sync hooks are available.

### Desktop experience and delivery

- Sidebar/content native shell with light and dark themes, custom icons, and native window chrome.
- Installers build for Apple Silicon and Intel macOS, Linux, and Windows.
- Release workflows package the unchanged `rem` CLI binary alongside desktop installers.
- A macOS/Linux curl installer installs both desktop and CLI; Windows publishes a separate CLI zip.
- CI covers TypeScript/unit tests, real-browser flows, Rust formatting, Clippy, Rust tests, frontend
  builds, security auditing, and cross-platform installer/CLI builds.

## Completed agent-first milestone

The four slices in
[`2026-08-09-agent-first-learning-loop-design.md`](superpowers/specs/2026-08-09-agent-first-learning-loop-design.md)
are implemented:

1. **Local drafts and agent command** — schema, collection operations, CLI/Tauri commands, and skill.
2. **Desktop draft inbox** — active front-first triage, edits, provenance, acceptance, and rejection.
3. **Shared Rust study module** — complete review behavior and FSRS moved behind `StudySession`.
4. **`rem study` TUI** — all-deck/single-deck terminal review over the shared module.

Current verification passes:

- 345 TypeScript/unit/real-browser tests;
- frontend typecheck and production build;
- Rust formatting and workspace Clippy with warnings denied;
- full Rust workspace tests, including temporary-SQLite study tests and pseudo-terminal CLI tests;
- `git diff --check`.

The frontend build retains only the existing Vite large-chunk advisory; there is no failing release
gate known in this worktree.

## Known gaps and incomplete behavior

These are real gaps in the resulting product, not features already delivered.

| Area | Current limitation |
|---|---|
| Draft feedback | Rejection reasons, acceptance/edit-rate summaries, and agent-readable quality feedback are not recorded. |
| Draft provenance | Rationale/source/producer metadata is not retained on the accepted card. |
| Duplicate help | Draft/card duplicate checks are exact normalized matches, not semantic suggestions. |
| Draft portability | Pending drafts are local-only and cannot be triaged from another machine or the TUI. |
| Habit support | No daily reminders or native notifications. |
| Review options | **Show answer timer** is stored but not rendered. |
| Related-card burying | The setting is stored but cannot be correctly enforced without a note/template/sibling model. |
| Organization | Decks are flat; namespaces, nested decks, and folders do not exist. |
| Navigation | The desktop sidebar cannot be collapsed. |
| Sync confidence | Automated coverage exists, but the documented real two-machine conflict/deletion smoke test is outstanding. |
| Extensibility | Git is the only user-facing sync backend; there is no backend selector or second provider. |
| Mobile | iOS artifacts exist, but there is no designed or supported mobile application. |
| Distribution | macOS is ad-hoc signed rather than notarized; Windows has no Authenticode signature. |
| Release | `v0.8.0` predates the draft inbox, shared study module, and terminal study loop; these changes need a release after merge. |

## Future direction

### Phase 1 — Ship and observe the agent-first loop

1. Review, merge, and release the completed draft → approval → study milestone.
2. Smoke-test an installed release: external agent draft, desktop acceptance, desktop/TUI study, and
   stale-grade protection against one shared collection.
3. Perform the documented two-machine Git-sync validation, including concurrent edits and deletion
   propagation.
4. Observe whether inbox age stays bounded and whether proposals are accepted with small edits.

**Outcome:** the complete local agent/human learning loop is trustworthy in a packaged release.

### Phase 2 — Improve learning quality and daily use from evidence

Only add draft feedback features after observing real use:

1. Optional rejection reasons and aggregate acceptance/edit-rate summaries.
2. Accepted-card provenance when it demonstrably helps trust or later quality analysis.
3. Semantic duplicate suggestions while preserving exact duplicate outcomes as the hard rule.
4. A TUI draft inbox or `$EDITOR` handoff if terminal-only users need approval away from desktop.
5. Native reminders, the existing answer-timer setting, and a collapsible sidebar.

**Outcome:** agents learn to propose fewer, better prompts and humans can sustain the habit with less
friction.

### Phase 3 — Strengthen the knowledge model

1. Introduce a note/template/sibling model only after defining user-facing value and migration.
2. Enforce related-card burying on top of that model.
3. Add nested decks or namespaces if flat decks limit real collections.
4. Extract a sync-provider seam only when adding a second remote backend.

**Outcome:** rem can express related cards and larger collections without compromising the simple
card/deck experience.

### Phase 4 — Add local intelligence

Start with rebuildable local embeddings rather than a scheduler or automatic author:

1. Semantic duplicate suggestions.
2. Search across decks.
3. Related-card suggestions during review.
4. Topic-aware interleaving experiments.

**Outcome:** rem helps users find overlap and connections without sending the collection to a
service.

### Phase 5 — Assisted source-to-draft authoring

The draft trust seam and provider-neutral CLI now exist. If users need generation inside rem, add a
small `CardGenerator` boundary that only creates reviewable drafts:

1. Import selected text or files.
2. Generate a small source-grounded candidate set.
3. Reuse the existing try/edit/accept/reject inbox.
4. Keep provider use explicit and never silently upload deck content.

An MCP adapter is justified only when a real client cannot invoke the local CLI reliably.

**Outcome:** source material can become useful recall prompts faster without weakening human control.

## Explicit non-goals for now

- Reintroducing SM-2 or using an LLM as the scheduler.
- Automatically accepting or scheduling agent-generated content.
- Treating proposal count as a success metric; zero good proposals remains valid.
- Bundling an agent runtime/model provider before the public CLI proves insufficient.
- Supporting the browser as a reduced product while core behavior depends on Rust and system Git.
- Building multiple storage/sync abstractions before a second real implementation exists.
- Expanding to mobile before desktop delivery and sync are dependable.

## Planning rule

Each future slice should have one user-visible outcome, a small design/spec, tests at the agreed
public seams, and an explicit release or validation step. Update this document when a phase
materially changes; keep implementation-specific investigation in dated files under
`docs/superpowers/`.
