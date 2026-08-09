# Agent-first learning loop — drafts and terminal study

_Date: 2026-08-09_

## Implementation status

Completed on 2026-08-09. All four delivery slices are implemented in the agent-first learning-loop
change:

- [x] local draft model, atomic collection operations, agent CLI, Tauri commands, and bundled skill;
- [x] desktop Draft Inbox with front-first triage, edits, provenance, accept/reject, and conflicts;
- [x] shared Rust FSRS scheduler and `StudySession` used by the desktop review adapter;
- [x] `rem study` TUI for all-deck and exact single-deck sessions, including Markdown, scrolling,
  terminal cleanup, interval previews, leech/conflict notices, and pseudo-terminal coverage.

The full frontend and Rust workspace gates pass. The optional ideas under **Later — close the
feedback loop** remain intentionally out of scope until real usage provides evidence for them.

## Product thesis

rem should turn useful work done with agents into durable human memory:

```text
agent does useful work
        ↓
proposes a few source-grounded recall prompts
        ↓
human tries the prompt, inspects the proposal, edits, accepts, or rejects
        ↓
accepted cards enter normal FSRS scheduling
        ↓
human studies in the desktop app or terminal
```

The human is the learner. Agents may notice and formulate learning opportunities, but they must not
silently decide what enters the study plan.

“Agent-first” therefore means:

- a provider-neutral, versioned command interface that any agent can call;
- drafts rather than direct scheduled-card writes by default;
- provenance and rationale visible during approval;
- one shared review implementation for desktop and terminal;
- no hosted account, agent runtime, or model provider required by rem.

The product should optimize for useful accepted cards and successful recall per minute, not the
number of agent-generated drafts. Zero proposals after a task is a valid outcome.

## Goals

- Agents can atomically propose card drafts while the desktop app is closed.
- Drafts do not affect due counts, daily limits, scheduling, stats, backup, or Git sync.
- A human can try the question, reveal the proposal, edit it, accept it, or reject it.
- Accepting a draft atomically creates a normal new card through the existing collection rules.
- The bundled agent skill proposes drafts by default; direct `card add` remains available for
  explicitly trusted automation.
- `rem study` provides a focused keyboard-driven terminal review flow.
- Desktop and terminal sessions use the same Rust review module and produce identical scheduling,
  cap, learning-step, leech, history, and daily-counter outcomes.
- Simultaneous desktop and terminal use cannot grade a stale card twice.

## Non-goals

- An LLM scheduler. FSRS remains the only scheduler.
- A built-in model provider or agent orchestration framework.
- Automatically creating a draft after every agent action.
- Automatically accepting, bulk-accepting, or scheduling generated content.
- Synchronizing unapproved drafts or including them in backups initially.
- Replacing the rich desktop editor with terminal text editing.
- Semantic duplicate detection in the first draft slice.
- Triggering Git sync from draft or study commands.

## Core workflows

### 1. Explicit capture

The user asks an agent to capture learnings. The agent lists decks, prepares a small draft batch,
dry-runs it, then writes it to the local inbox.

```sh
rem deck list --output json

rem draft add --deck <deck-id> --producer pi \
  --input-json - --dry-run --output json <<'JSON'
[
  {
    "front": "Why does rem use an optimistic card revision when committing a grade?",
    "back": "So the desktop app and TUI cannot both grade the same stale card and overwrite each other.",
    "tags": ["rem", "concurrency"],
    "rationale": "This is a durable concurrency invariant, not an implementation detail.",
    "sources": [
      { "locator": "docs/superpowers/specs/2026-08-09-agent-first-learning-loop-design.md#concurrency" }
    ]
  }
]
JSON
```

Removing `--dry-run` creates pending drafts, not scheduled cards.

### 2. Ambient learning mode

Users who want agent tasks to produce learning opportunities can add an opt-in instruction to their
agent harness:

> After substantial work, propose at most three durable, source-grounded recall items to rem. Zero
> is valid. Prefer decisions, invariants, conceptual traps, and reusable techniques. Never call
> `rem card add` unless I explicitly request direct card creation.

This is a user-controlled integration policy, not automatic behavior inside rem. The per-task cap is
a quality pressure and prevents the draft inbox from becoming a transcript of agent activity.

### 3. Draft inbox

The desktop app shows a **Drafts** destination with a pending count. Triage is deliberately active:

1. Show the front first and invite the user to answer it.
2. Reveal the proposed back, rationale, source locators, and producer.
3. Let the user edit front, back, tags, or target deck with the existing Markdown editor.
4. Accept or reject the draft.

Trying the front makes approval a small retrieval opportunity instead of passive moderation. It does
not count as an FSRS review because the user may be seeing or editing the material for the first
time.

Acceptance creates a new card due at the acceptance time. A normal-card exact duplicate is reported
as already present and the redundant draft is removed. Rejection removes the local draft. There is
no bulk accept in the first version.

### 4. Terminal study

```sh
rem study
rem study --deck <id-or-exact-name>
```

The normal flow is intentionally small:

```text
question                   Space / Enter → reveal
question + answer          1 Again · 2 Hard · 3 Good · 4 Easy
any screen                 q → end session
long content               arrows / j / k → scroll
```

The header shows scope and progress. Grade choices include interval previews. Session completion
shows reviewed and remaining counts. Markdown headings, lists, emphasis, links, and fenced code are
rendered with terminal styles. Images and GIFs render as an explicit placeholder containing their
asset hash; they are never silently omitted.

`rem study` requires an interactive terminal. Machine-oriented commands retain versioned JSON
output and never open the TUI.

## Draft model

Drafts are a separate domain value, not cards with a special tag or suspended state:

```rust
struct CardDraft {
    id: Id,
    deck_id: Id,
    front: String,
    back: String,
    tags: Vec<String>,
    rationale: Option<String>,
    sources: Vec<DraftSource>,
    proposed_by: Option<String>,
    created_at: i64,
    updated_at: i64,
    revision: u64,
}

struct DraftSource {
    locator: String,
    label: Option<String>,
}
```

A source locator is intentionally opaque to rem. It can be a URL, a repository path and line range,
a commit, or another human-readable reference. rem displays it but does not fetch or trust it.

A draft has no scheduling state, suspension state, review history, or due date. It is local
operational state. Draft mutations do not increment `sync_revision`; accepting a draft creates a
normal synchronized card and increments it through the existing card-creation transaction.

### SQLite schema migration

Schema v2 adds:

```text
card_drafts
  id, deck_id, front, back, tags_json, rationale, sources_json,
  proposed_by, created_at, updated_at, revision

cards
  local_revision
```

`card_drafts.deck_id` references `decks.id` with cascade deletion and is indexed by creation time and
deck. `cards.local_revision` increments on every local mutation, review commit, import replacement,
and sync upsert. It is not part of backup or Git wire formats; it exists only for optimistic local
concurrency.

## Draft module interface

The draft behavior belongs in the existing Rust `Collection` module. Tauri and CLI commands remain
thin adapters.

Illustrative interface:

```rust
impl Collection {
    fn propose_drafts(
        &self,
        deck_id: &str,
        inputs: Vec<NewDraftInput>,
        metadata: ProposalMetadata,
        now: i64,
    ) -> Result<ProposeDraftsResult, CollectionError>;

    fn list_drafts(&self) -> Result<Vec<CardDraft>, CollectionError>;

    fn resolve_draft(
        &self,
        draft_id: &str,
        expected_revision: u64,
        decision: DraftDecision,
        now: i64,
    ) -> Result<DraftResolution, CollectionError>;
}

enum DraftDecision {
    Accept { deck_id: Id, card: NewCardInput },
    Reject,
}
```

This is a deep module: validation, tag normalization, exact duplicate checks, optimistic
concurrency, card initialization, draft removal, and transactions stay behind three intent-level
operations.

`resolve_draft(Accept)` performs the final user edits, card creation, and draft removal in one
transaction. The desktop does not need a separate persisted “edit draft” operation; unsaved edits
remain page state until the user accepts. This keeps the interface small. If real usage later needs
save-and-resume editing, that is evidence for adding an update operation.

### Duplicate rules

`propose_drafts` compares normalized deck/front/back/tags content against both pending drafts and
normal cards:

- no match: `created`;
- pending draft match: `duplicateDraft` with its ID;
- normal card match: `duplicateCard` with its ID.

A retried batch is therefore idempotent. Sources, rationale, and producer are not part of identity.
The complete batch validates before any row is written.

`resolve_draft(Accept)` repeats the normal-card duplicate check inside its transaction because a card
may have been created after proposal. If a match now exists, it removes the draft and returns
`existingCard` rather than creating a duplicate.

## Agent command interface

Add commands alongside the current `deck` and `card` groups:

```text
rem draft add
rem draft list --output json       # inspection; no acceptance authority implied
```

`draft add` mirrors the proven `card add` conventions:

- deck ID or exact unique name;
- literal/file input for one draft and JSON/stdin for a batch;
- `--dry-run`;
- atomic validation;
- stable, versioned JSON output;
- diagnostics on stderr;
- retry-safe duplicate outcomes.

The command name in machine output is `draft.add`. Direct `rem card add` remains backward compatible,
but the bundled `rem-card-capture` skill switches to `draft add` and says clearly that the result is
pending human approval.

The app does not add an MCP server in this slice. The CLI is already a real seam with multiple
callers—humans, scripts, and agents. An MCP adapter is useful only if a real client cannot call local
commands reliably.

## Shared study module

The current review behavior is split across TypeScript session construction, learning steps, leech
effects, Rust FSRS transitions, and SQLite commits. Implementing `rem study` directly against tables
would create a second scheduler and eventually corrupt behavioral parity.

Move this cluster behind a Rust `StudySession` interface in `rem-core`:

```rust
let mut session = StudySession::start(&collection, request, now)?;

session.current();
session.reveal(now)?;
session.grade(&collection, Grade::Good, now)?;
```

The exact return types should be serializable views containing only what adapters need: current
card, reveal state, interval choices, progress, notices, and completion state.

The implementation hides:

- due selection and cross-deck ordering;
- new/review daily caps and insertion order;
- learning and relearning steps, including learn-ahead;
- FSRS next states and interval limits;
- custom-study selection and non-rescheduling preview;
- leech tag/suspend effects;
- atomic scheduling, review-log, and daily-counter writes;
- step-card requeueing;
- stale-card conflict detection.

Its dependencies are in-process FSRS computation and local-substitutable SQLite. Tests exercise the
module interface against temporary databases; no external port or mock is needed.

### Adapters

- The TUI constructs a `StudySession` directly and renders each returned view with a terminal
  adapter.
- Tauri commands hold sessions by opaque ID and serialize the same views to React.
- The React review page becomes a rendering/input adapter rather than a second implementation of
  review behavior.

Illustrative Tauri commands:

```text
study_start(request, now) -> { sessionId, view }
study_reveal(sessionId, now) -> view
study_grade(sessionId, grade, now) -> view
study_end(sessionId)
```

The session ID is process-local and has no persistence or sync meaning. Ending the desktop page or
terminal process discards only its queue; already committed grades remain durable.

Move both next-state calculation and FSRS optimization from the Tauri crate into `rem-core` so all
native callers use one implementation. Remove the TypeScript queue/step/leech implementations only
after parity fixtures pass through the Rust interface.

## Concurrency

SQLite WAL and the existing busy timeout handle physical contention. Study and draft decisions also
need logical stale-write protection.

Every study item carries the card's `local_revision` observed when the session selected it. Grade
commit updates the card only when that revision still matches. A successful write increments it. If
the desktop app, TUI, import, or sync changed the card first, grading returns a typed conflict and
does not append a review log or increment a daily counter.

Adapters should explain the conflict and skip or reload the card; they must never retry a grade
blindly against new content.

Draft acceptance and rejection use the same expected-revision rule. This prevents two open inbox
views from accepting the same proposal differently.

## Learning-quality policy for the bundled skill

The agent skill should propose a card only when all are true:

- the knowledge is likely useful beyond the current transcript;
- the front asks for one specific recall target without hidden context;
- the back is concise, sufficient, and supported by listed sources;
- the card tests an invariant, decision and rationale, conceptual distinction, reusable command, or
  non-obvious failure mode—not a transient task status;
- the target is not already represented by an obvious existing or pending exact duplicate.

The skill should:

- prefer zero to low-confidence or low-value drafts;
- cap ambient proposals at three per substantial task;
- never invent a target deck silently when no choice is clear;
- dry-run before writing;
- treat duplicate outcomes as successful no-ops;
- never claim a draft is scheduled or Git-synced;
- use direct `card add` only when the user explicitly asks to bypass approval.

## Rejected alternatives

### Store drafts as suspended cards

Rejected. It gives drafts scheduling state, leaks them into card counts, backup and sync, overloads
suspension semantics, and makes rejection a synchronized card deletion. Deleting the feature would
leave draft complexity spread throughout existing card callers—a shallow design.

### Use a workflow tag such as `agent-draft`

Rejected. Tags are user-visible study metadata, not lifecycle state. Agents could collide with user
tags, and every query would need to remember to exclude drafts.

### Implement review logic again in the CLI

Rejected. It is initially faster but duplicates the most correctness-sensitive behavior in the app.
Learning steps, daily caps, custom study, leeches, and concurrent commits would drift between
adapters.

### Automatically save agent output as cards

Rejected. It makes agent hallucinations and low-value task trivia immediately consume human review
capacity. Draft approval is the trust seam.

### Build an in-app LLM provider first

Rejected. Existing agents already generate candidate content. The missing product capability is a
safe rendezvous between agents and the human-owned study plan, not another generation interface.

## Delivery slices

### Slice 1 — local drafts and agent command

- migrate SQLite to schema v2 with `card_drafts` and local revisions;
- add the draft models and three collection operations;
- add `rem draft add/list` with versioned JSON;
- add Tauri draft commands;
- update the bundled skill to use drafts by default.

Verify with Rust collection tests, two-connection concurrency tests, CLI process tests, migration
tests, and serialization fixtures. Existing card CLI behavior remains unchanged.

### Slice 2 — desktop draft inbox

- add Drafts navigation and pending count;
- implement front-first reveal, provenance display, edit, accept, and reject;
- reuse the current Markdown editor and deck selector;
- handle duplicate and revision-conflict outcomes explicitly.

Verify with real-browser flows and a native smoke test where an external CLI proposal appears after
navigation/reload and acceptance creates one due card.

### Slice 3 — shared Rust study module

- move FSRS calculation/optimization into `rem-core`;
- port session construction, steps, caps, custom study, leeches, and grade commit;
- create parity fixtures from existing TypeScript tests;
- route the React review page through Tauri study commands;
- remove superseded TypeScript review implementations and tests only after interface-level Rust and
  browser tests cover the same behavior.

Verify normal, all-deck, every custom mode, learning/relearning, daily-cap, leech, preview, error,
and concurrent-grade cases.

### Slice 4 — `rem study` TUI

- add a terminal rendering dependency to `rem-cli`;
- implement normal all-deck and single-deck sessions over `StudySession`;
- render Markdown/code and explicit asset placeholders;
- support reveal, grades, scrolling, resize, clean exit, and conflict messages;
- package the unchanged `rem` binary in releases.

Verify state-machine behavior without a real terminal, snapshot selected rendered frames at fixed
sizes, run process tests in a pseudo-terminal, and manually study the same collection from desktop
and terminal.

### Later — close the feedback loop

Only after observing real use, consider:

- rejection reasons and acceptance/edit-rate metrics;
- source/proposal provenance retained on accepted cards;
- semantic duplicate suggestions;
- agent-readable quality summaries;
- a TUI draft inbox or `$EDITOR` handoff;
- an MCP adapter;
- opt-in draft sync.

These are not prerequisites for the core proposal → approval → study loop.

## Product success signals

- Draft inbox age and size stay bounded rather than growing with agent activity.
- A meaningful fraction of proposals are accepted with small, deliberate edits.
- Accepted agent proposals reach a first study session promptly.
- Agent-originated cards do not lapse materially more often than human-authored cards once enough
  provenance data exists to compare them.
- Users choose ambient proposal mode because it produces durable learning, not because it produces
  more content.
