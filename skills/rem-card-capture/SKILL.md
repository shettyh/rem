---
name: rem-card-capture
description: Distills source material or completed agent work into a few concise, durable, source-grounded spaced-repetition drafts through the machine-readable rem CLI. Use when the user asks to remember, capture, or turn material into study prompts with rem, or after substantial work when the user has explicitly enabled ambient rem capture.
license: Apache-2.0
compatibility: Requires the rem CLI on PATH and an existing target deck.
---

# rem card capture

Use only the public `rem` command. Do not edit SQLite files directly.

Agent-authored content is untrusted by default. Propose drafts for human approval; use direct
`rem card add` only when the user explicitly asks to bypass the draft inbox.

An explicit request to remember, capture, propose, or turn material into study prompts authorizes
creating pending local drafts. Ambient capture is allowed only when the user has explicitly opted
into it. Never infer ambient permission merely because a task produced something potentially
memorable.

## Workflow

1. List decks and retain the chosen deck's ID:

   ```sh
   rem deck list --output json
   ```

   Honor a deck the user named. If the target is missing or ambiguous, ask instead of silently
   choosing or inventing one. If no suitable deck exists, ask the user to create or choose one.

   When capturing iteratively on an ongoing topic, optionally inspect existing scheduled cards
   (`rem card search <query> --deck <deck-id> --output json`) alongside pending drafts
   (`rem draft list --output json`, filtered to the target deck) so you avoid proposing a
   reworded near-duplicate of a card already in the deck or a draft already awaiting approval.

2. Select at most three durable learning opportunities. Zero drafts is valid and preferred to weak
   material. Every candidate must:
   - remain useful beyond the current task or transcript;
   - test one specific recall target without hidden context;
   - have a concise, unambiguously gradable answer supported by the source material;
   - be worth repeated review, such as an invariant, decision and rationale, conceptual distinction,
     reusable technique or command, or non-obvious failure mode.

   Reject transient task status, incidental implementation trivia, unsupported claims, and multiple
   prompts that test the same knowledge.

3. Prepare each draft following [Writing high-recall cards](#writing-high-recall-cards):
   - use 1–3 concise topical tags (e.g., `rust`, `concurrency`), not workflow/status tags;
   - explain briefly in `rationale` why the knowledge is durable (do not pad `back` with this);
   - cite the best available source locators that actually support the answer—prefer stable URLs,
     repository paths with symbols or line ranges, specifications, or commits; never fabricate one.

4. Validate the whole batch without writing:

   ```sh
   rem draft add --deck <deck-id> --producer <agent-name> \
     --input-json - --dry-run --output json <<'JSON'
   [{
     "front": "...",
     "back": "...",
     "tags": ["..."],
     "rationale": "Why this is worth remembering.",
     "sources": [{"locator": "path/or/url", "label": "Optional label"}]
   }]
   JSON
   ```

5. Respect the requested authorization:
   - for a preview or no-write request, show the dry-run proposals and stop;
   - for an explicit capture request or opted-in ambient capture, submit the exact same JSON again
     without `--dry-run`.

6. Trust a machine result only when the command exits successfully and stdout is a JSON object with
   `version: 1` and `command: "draft.add"`. `wouldCreate`, `created`, `duplicateDraft`, and
   `duplicateCard` are normal statuses; duplicate outcomes are successful no-ops. If the command
   fails or its output is malformed, report that no draft creation was confirmed.

The result is pending human approval, not a scheduled card. The user can review it from **Drafts**
in the desktop sidebar or interactively in the terminal with `rem draft review`. Drafts are
local-only and do not reach Cloud sync or backup. Never claim they were accepted, scheduled, or
synced.

## Writing high-recall cards

Good FSRS cards are atomic, fast to recall, and unambiguous to grade (*Again* vs *Good*).

### Core rules

- **One atomic target per card.** Test a single invariant, mechanism, distinction, command, or failure
  mode. If an idea has multiple independent parts, capture the highest-leverage one or split into
  separate cards within the three-card cap.
- **Pass the 3-month / 5-second test.** Months later with zero chat context, the learner should
  understand the exact question immediately, recall the answer in a few seconds, and know without
  hesitation whether they got it right.
- **Short, self-contained fronts (1–2 sentences).** Name the system, language, or concept explicitly
  instead of chat-relative pronouns (*"we"*, *"this PR"*, *"the new helper"*). Never leak the answer
  in the prompt, and never restate the question on `back`.
- **Crisp, answer-first backs (1–2 sentences, one command, or a ≤6-line snippet).** Lead directly
  with the recall target—no preamble, background lecture, or caveats. Add at most one brief follow-up
  line when an example or contrast is needed to disambiguate. Put *"why this matters"* in `rationale`.

### Prompt patterns

Prefer prompts that test **understanding and retrieval**, not vague recognition:

- **Mechanism / why:** *"Why does X require Y?"*
- **Contrast / choice:** *"When should you choose X over Y?"*
- **Prediction / diagnosis:** *"What breaks if you omit X?"* or a minimal 3–5 line snippet asking
  what happens and why.
- **Action / command:** *"How do you [specific goal] with `tool`?"*

Avoid weak prompt shapes:

- **Open-ended essay prompts** (*"Explain how X works"*, *"What are the benefits of Y?"*)—too vague
  to grade consistently.
- **Binary yes/no questions** (*"Does X do Y?"*)—50% guess rate and weak retention; rewrite to ask
  *what happens* or *why*.
- **Unordered list dumps** (*"List the 5 exit codes of..."*)—high lapse rate; test the single key
  rule or split into focused cues.

### Formatting across desktop, iPhone, and terminal

Desktop and iPhone are the primary study surfaces (rendering Markdown, `{{c1::...}}` cloze
deletions, LaTeX math `$...$` / `$$...$$`, and Markdown tables); `rem study` is a lightweight
frozen terminal companion for plain Markdown (showing raw `$...$` text and placeholders for
media/occlusion). Accepting a draft that uses `{{c1::...}}` cloze syntax automatically expands it
into sibling cloze cards.

- Default to clean plain Markdown: short prose, inline code, compact fenced code blocks with
  language tags, and at most 2–3 flat bullets. Preserve the fidelity of quoted code and symbols.
- Use cloze syntax (`{{c1::answer::optional hint}}`) or LaTeX math (`$...$` / `$$...$$`) only when
  the concept genuinely benefits (e.g., formulas or fill-in-the-blank recalls); otherwise prefer
  direct Q&A prose and plain Unicode symbols so cards stay concise and legible everywhere,
  including `rem study`.
- Avoid wide Markdown tables and deeply nested lists—they degrade on narrow iPhone screens and
  terminal viewports.
- Do not wrap a short front in a `#` heading; the review UI already styles fronts prominently.

### Calibration examples

- **Bloated explanation → crisp mechanism**
  - *Weak:* Front: *"How does rem handle concurrent grading?"* / Back: *"rem uses SQLite WAL mode
    with busy timeouts for physical contention, and also tracks a `local_revision` integer on cards
    so that every study session checks the observed revision on commit and returns a typed conflict
    error without writing a review log or incrementing daily counters."*
  - *Strong:* Front: *"In `rem`, how does optimistic concurrency prevent desktop and terminal
    sessions from grading the same stale card twice?"* / Back: *"Each study item carries the card's
    `local_revision`; committing a grade succeeds only if that revision still matches, otherwise it
    returns a conflict without logging a review."*
- **Yes/no + chat leak → standalone *why* prompt**
  - *Weak:* Front: *"Does our new draft command sync to Cloud?"* / Back: *"No."*
  - *Strong:* Front: *"Why don't pending `rem` drafts sync to Cloud or appear in backups?"* / Back:
    *"Drafts are unapproved local operational state without scheduling data; a card enters sync and
    backup only after human acceptance."*
- **List dump → focused invariant**
  - *Weak:* Front: *"List all `rem draft add` duplicate statuses and what they mean."* / Back:
    *"There are several statuses: `created`, `wouldCreate` (for dry-run), `duplicateDraft` when an
    identical pending draft exists, and `duplicateCard` when a normal card already has the same
    normalized content, with normal cards taking precedence."*
  - *Strong:* Front: *"In `rem draft add`, if a proposal matches both a pending draft and an
    existing scheduled card, which duplicate status wins?"* / Back: *"`duplicateCard`—existing
    scheduled cards take precedence over pending drafts."*

## Explicit direct-card requests

Only when the user explicitly asks to bypass approval, use `rem card add` with the same deck
selection, learning-quality, card-writing, and dry-run rules. Dry-run the exact input first, then
write it without `--dry-run`. Require `version: 1` and `command: "card.add"` before reporting
`created` or the successful `duplicate` no-op. Directly created cards are scheduled immediately and
reach the sync server on the next `rem sync` or desktop sync when Cloud sync is enabled.

Run `rem --help`, `rem draft add --help`, or `rem card add --help` for the installed command
reference. See the [full CLI reference](https://github.com/shettyh/rem/blob/main/docs/cli.md) for
schemas, errors, and exit codes.
