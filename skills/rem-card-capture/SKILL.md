---
name: rem-card-capture
description: Distills source material or completed agent work into a few durable, source-grounded spaced-repetition drafts through the machine-readable rem CLI. Use when the user asks to remember, capture, or turn material into study prompts with rem, or after substantial work when the user has explicitly enabled ambient rem capture.
license: Apache-2.0
compatibility: Requires the rem CLI on PATH and an existing target deck.
---

# rem card capture

Use only the public `rem` command. Do not edit SQLite or Git files directly.

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

2. Select at most three durable learning opportunities. Zero drafts is valid and preferred to weak
   material. Every candidate must:
   - remain useful beyond the current task or transcript;
   - test one specific recall target without hidden context;
   - have a concise but sufficient answer supported by the source material;
   - be worth repeated review, such as an invariant, decision and rationale, conceptual distinction,
     reusable technique or command, or non-obvious failure mode.

   Reject transient task status, incidental implementation trivia, unsupported claims, and multiple
   prompts that test the same knowledge.

3. Prepare each draft:
   - preserve useful Markdown and the fidelity of quoted code;
   - use a small set of topical tags, not workflow/status tags;
   - explain briefly in `rationale` why the knowledge is durable;
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
in the desktop sidebar. Drafts are local-only and do not reach Git sync or backup. Never claim they
were accepted, scheduled, or synced.

## Explicit direct-card requests

Only when the user explicitly asks to bypass approval, use `rem card add` with the same deck
selection, learning-quality, and dry-run rules. Dry-run the exact input first, then write it without
`--dry-run`. Require `version: 1` and `command: "card.add"` before reporting `created` or the
successful `duplicate` no-op. Directly created cards are scheduled immediately and reach Git on the
desktop app's next sync.

Run `rem --help`, `rem draft add --help`, or `rem card add --help` for the installed command
reference. See the [full CLI reference](https://github.com/shettyh/rem/blob/main/docs/cli.md) for
schemas, errors, and exit codes.
