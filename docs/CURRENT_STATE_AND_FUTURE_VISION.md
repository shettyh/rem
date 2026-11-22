# rem — Current State & Future Vision

_Last reviewed: 2026-11-22 · based on `v0.18.0` plus FSRS-7, macOS signing/notarization, bundled single-install CLI, Tauri auto-updater, and Homebrew tap_

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
human studies in the desktop app, terminal, or iPhone
```

The product should make five things especially easy:

1. **Capture** a durable card from the desktop app, terminal, or iPhone without interrupting the user's work.
2. **Turn agent work into learning opportunities** without silently scheduling generated content.
3. **Review** the right cards in a calm, fast session across desktop, terminal, and iPhone.
4. **Understand and tune** the study plan without requiring scheduling expertise.
5. **Own and move the data** without depending on an account, hosted service, or model provider.

### Product principles

- **The human is the learner and approval authority.** Agents may notice and formulate learning
  opportunities, but generated content remains a draft until a human accepts it.
- **Local-first and user-owned.** SQLite is primary. Backup and sync remain inspectable and portable.
- **Correct model for the job.** FSRS schedules reviews; an LLM does not.
- **Provider-neutral agent integration.** A versioned CLI is the public seam. rem does not require or
  embed an agent runtime or model provider.
- **One review implementation.** Desktop, terminal, and iOS adapters use the same Rust `StudySession`
  so scheduling, caps, learning steps, leeches, and concurrency behavior cannot drift.
- **Simple before clever.** Prefer a dependable proposal → approval → study loop over speculative
  automation.
- **Stable seams.** Scheduling, persistence, sync transport, FFI, and agent-facing operations stay
  behind small intent-level interfaces.

## Current state

rem is a local-first spaced-repetition suite built around a shared Rust core (`rem-core` and
`rem-sync`):

- **Desktop app:** Tauri v2 desktop application with React, TypeScript, and Vite rendering the
  webview; Rust owns native persistence, scheduling, study sessions, and cloud sync. The browser
  build is an internal webview/test adapter, not a supported browser product.
- **Terminal CLI:** Companion `rem` CLI for agent/script capture, draft ingestion, cloud sync, and
  interactive terminal study (`rem study`).
- **iOS companion:** Native SwiftUI iPhone app (`ios/`) backed by `rem-core` and `rem-sync` via
  UniFFI (`src-tauri/crates/rem-ffi` packaged as `RemCore.xcframework`).

### Agent-first capture and draft approval

- `rem deck list`, `rem card add`, `rem draft add`, and `rem draft list` work while the desktop app is
  closed and support stable versioned JSON for agents and scripts.
- Card and draft batches validate atomically, resolve deck IDs or exact names, normalize tags,
  support dry runs, and return retry-safe exact-duplicate outcomes.
- Agent proposals are stored as a separate `CardDraft` domain value with rationale, opaque source
  locators, producer, timestamps, and optimistic revision.
- Pending drafts have no scheduling state, due date, review history, or suspension state. They do not
  affect counts, stats, backup, or cloud sync.
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

- Desktop full-screen add/edit experience using a TipTap WYSIWYG-Markdown editor; Markdown remains
  the stored source of truth.
- Formatting toolbar for headings, emphasis, lists, links, inline code, and fenced code blocks.
- Syntax-highlighted code when editing and reviewing (desktop and iOS).
- Image and GIF ingestion through file picker, paste, drag-and-drop (desktop), and iOS Photos picker.
- Content-addressed binary assets render in cards and travel through backup and cloud sync (via R2).
- User-defined tags, tag chips, and per-deck tag filtering.
- Create, edit, delete, suspend, and restore cards.

### Decks and study

- Flat deck organization with colors, live due counts, deck progress rings, and cross-deck study.
- FSRS-7 calculation and optimization live in `rem-core`; SM-2 has been removed.
- A shared Rust `StudySession` owns due selection, daily caps, learning/relearning steps, FSRS
  choices, custom study, leeches, review persistence, undo snapshots, counters, requeueing, and
  stale-card conflicts.
- The desktop review page, `rem study` TUI, and iOS `ReviewScreen` are all thin adapters over the
  same `StudySession` implementation.
- `rem study` reviews all decks; `rem study --deck <id-or-exact-name>` reviews one deck. The
  interactive TUI supports Space/Enter reveal, 1–4 grading, arrow/j/k scrolling, resize, clean `q`
  exit, interval previews, progress, completion counts, leech notices, stale-card messages, and
  explicit image/GIF placeholders containing asset hashes.
- Custom desktop study modes include study ahead, additional new cards, forgotten cards, and
  non-rescheduling new-card preview.
- Per-deck controls cover desired retention, learning/relearning steps, new/review limits, insertion
  order, minimum/maximum intervals, leech behavior, and personalized FSRS weights.

### iOS companion app (v1)

- Native SwiftUI iPhone app (`ios/RemApp/`, iOS 17+) following the Quiet Paper Light/Dark visual
  system matching desktop design tokens.
- Navigation uses 4 tabs (`Today`, `Decks`, `Stats`, `Settings`) plus a dedicated center `+` Quick
  Capture button, hiding chrome during full-screen review.
- **First-run & local-first:** No startup login wall — users can start locally on device SQLite,
  connect to `rem Cloud`, or configure a self-hosted sync server URL.
- **Focused review:** Tap-to-reveal + 4-button FSRS-6 grade dock with live intervals, optional
  haptics, adjustable card text size (`S / M / L`), in-review card editing, undo last grade, card
  suspension, dark syntax-highlighted code blocks, and offline `asset:<hash>` image/GIF rendering
  with a pinch-to-zoom lightbox.
- **Quick capture & deck browsing:** Fast card creation/editing with Write/Preview toggle, Markdown
  keyboard toolbar, tag editor, iOS Photos image attachment (SHA-256 stored in SQLite), and deck/tag
  filtering.
- **Stats, habits & sync:** 7D/30D/90D Stats dashboard with per-deck breakdown, Apple/Google OAuth
  via `ASWebAuthenticationSession` + iOS Keychain (`FfiCredentialStore`), automatic and pull-to-refresh
  Cloud Sync, local daily reminders via `UNUserNotificationCenter` (scheduled when due > 0), and
  JSON backup export via the iOS share sheet.

### Insight and history

- Review history records FSRS-effective grades.
- Stats dashboards (desktop and iOS) support global or per-deck scope, 7/30/90-day ranges, review
  activity, recall rate, streak, active days, grade distribution, and deck breakdown.
- Daily-cap counters are local to each device; review history is included in backup and sync.

### Data, backup, and sync

- Native desktop, CLI, and iOS operations use SQLite through `rem-core`; the TypeScript `Storage`
  interface keeps React independent of native commands, and Dexie remains a browser-test adapter.
- SQLite schema v3 contains local-only drafts, per-card optimistic `local_revision` values,
  per-domain Hybrid Logical Clocks (HLCs), and local `sync_meta` / `sync_outbox` / `synced_assets`
  bookkeeping tables.
- Deck-scoped JSON export/import preserves scheduling and review history, with confirmation before a
  same-named deck is replaced.
- Legacy Git sync has been removed in favor of optional Cloud sync (`Local only` vs. `Cloud sync`).
  The shared `rem-sync` crate talks to the open-source Cloudflare Workers + SQLite Durable Objects +
  D1 + R2 backend in `sync-server/` (either hosted `rem Cloud` or a self-hosted deployment).
- Hosted `rem Cloud` supports server-side entitlement gating (`REQUIRE_SUBSCRIPTION`), early-access
  approvals in D1, and a same-origin `/cloud` waitlist page.
- Google and Apple OAuth (PKCE + short-lived JWTs + rotating refresh tokens) store credentials in
  the OS Keychain (desktop `keyring` and iOS Security framework via `rem-ffi`); commutative
  per-domain HLC merges and tombstones resolve concurrent edits and reviews automatically without
  halting conflict dialogs.
- Unapproved drafts and local daily-cap counters are intentionally excluded from backup and cloud sync.
- Manual sync from Settings, automatic sync triggers (startup/foreground, debounced writes,
  focus/online, periodic, post-review), account deletion, and `rem sync` CLI commands are implemented.

### Desktop experience and delivery

- Collapsible icon-rail sidebar and native content shell with light and dark themes, dense cards
  list, custom icons, and native window chrome.
- Installers build for Apple Silicon and Intel macOS (Developer ID signed & notarized), Linux, and Windows.
- The `rem` CLI binary is bundled directly inside desktop packages (`rem.app/Contents/MacOS/rem` on macOS) so Homebrew (`brew install --cask shettyh/tap/rem`) and `install.sh` provide a single installation that stays updated in lockstep with the desktop app; standalone CLI archives are also published on GitHub Releases.
- Signed in-app auto-updates (`tauri-plugin-updater`) check `latest.json` on GitHub Releases and update both desktop and linked CLI together.
- CI covers TypeScript/unit tests, real-browser flows, Rust formatting, Clippy, Rust workspace tests,
  frontend builds, iOS `RemCore.xcframework` + `RemAppTests` Simulator tests, security auditing,
  cross-platform desktop/CLI release builds, and cloud-signed iOS TestFlight uploads.

## Completed milestones

### 1. Agent-first learning loop

All four slices in
[`2026-08-09-agent-first-learning-loop-design.md`](superpowers/specs/2026-08-09-agent-first-learning-loop-design.md)
are implemented:

1. **Local drafts and agent command** — schema, collection operations, CLI/Tauri commands, and skill.
2. **Desktop draft inbox** — active front-first triage, edits, provenance, acceptance, and rejection.
3. **Shared Rust study module** — complete review behavior and FSRS moved behind `StudySession`.
4. **`rem study` TUI** — all-deck/single-deck terminal review over the shared module.

### 2. Cloud Sync & iOS Companion

All four slices in
[`2026-11-21-cloudflare-cloud-sync-and-mobile-design.md`](superpowers/specs/2026-11-21-cloudflare-cloud-sync-and-mobile-design.md)
and [`2026-11-21-ios-companion-design.md`](superpowers/specs/2026-11-21-ios-companion-design.md)
are implemented:

1. **`rem-core` sync engine** — SQLite schema v3, HLCs, 7 commutative op types, transactional
   `sync_outbox`, `apply_remote_ops`, and bootstrap generator.
2. **Cloudflare `sync-server` + `rem-sync`** — Worker, per-user SQLite `UserSyncDO`, R2 content-addressed
   asset routes, D1 auth/entitlements, and Rust `rem-sync` client.
3. **Auth, Desktop UI, CLI sync & Git sync retirement** — Apple + Google OAuth, OS Keychain, hosted
   `rem Cloud` default + self-hosted server URL, `/cloud` waitlist & entitlement gate, `rem sync` CLI,
   and clean removal of legacy Git sync.
4. **Native SwiftUI iOS Companion (v1)** — `rem-ffi` UniFFI crate, `RemCore.xcframework` build script,
   and full SwiftUI iPhone app covering onboarding, Today, Decks, Review, Quick Capture, Stats,
   Settings, Cloud OAuth, local reminders, and backup export.

## Known gaps and incomplete behavior

These are real gaps in the resulting product, not features already delivered.

| Area | Current limitation |
|---|---|
| Draft feedback | Rejection reasons, acceptance/edit-rate summaries, and agent-readable quality feedback are not recorded. |
| Draft provenance | Rationale/source/producer metadata is not retained on the accepted card. |
| Duplicate help | Draft/card duplicate checks are exact normalized matches, not semantic suggestions. |
| Draft portability | Pending drafts are local-only and cannot be triaged from another machine or the TUI. |
| Habit support | No daily reminders or native notifications on desktop (iOS supports local daily reminders via `UNUserNotificationCenter`). |
| Review options | **Show answer timer** is stored but not rendered. |
| Related-card burying | The setting is stored but cannot be correctly enforced without a note/template/sibling model. |
| Organization | Decks are flat; namespaces, nested decks, and folders do not exist. |
| Cloud sync v1.1 | Server-side op compaction (`/v1/bootstrap`), real-time WebSocket pokes, automated billing webhooks, and an optional non-Cloudflare self-host server are deferred beyond initial early access. |
| iOS companion v1.1 | WidgetKit Home/Lock Screen widgets, Share Sheet capture extension, per-deck FSRS settings/optimizer UI, and Custom Study modes are deferred past v1. |
| CI & Distribution | Windows installers and CLI binaries do not yet have an Authenticode signature; public App Store release (beyond TestFlight) is not yet submitted. |

## Future direction

### Phase 1 — Ship, distribute, and validate the multi-device loop

1. Cut the next desktop/CLI release including Cloud Sync and the desktop/iOS UX polish pass.
2. Smoke-test installed releases across all surfaces: external agent draft → desktop acceptance →
   desktop/TUI study, and physical iPhone ↔ desktop/CLI Cloud Sync convergence (offline grading,
   photo/asset sync, reconnect, and force-quit recovery).
3. Wire iOS builds/tests into CI and set up Apple Developer signing for TestFlight / App Store
   distribution (and macOS notarization / Windows Authenticode signing when ready).
4. Observe whether draft inbox age stays bounded and whether agent proposals are accepted with small
   edits.

**Outcome:** the complete local + multi-device agent/human learning loop is trustworthy in packaged
releases across desktop, terminal, and iPhone.

### Phase 2 — Cloud Sync v1.1, iOS v1.1, and daily habit ergonomics

Prioritize high-leverage multi-device polish and evidence-backed habit features:

1. **Cloud Sync v1.1:** server-side op compaction (`/v1/bootstrap` + gzipped R2 snapshots),
   Hibernatable WebSocket real-time pokes, automated billing webhooks (and optional non-Cloudflare
   VPS/Docker server if self-hosters need it).
2. **iOS Companion v1.1:** WidgetKit Home/Lock Screen widgets (with App Group SQLite migration),
   Safari/system Share Sheet capture extension, per-deck FSRS settings & weight optimizer UI, and
   Custom Study modes.
3. **Draft quality & desktop habit polish** (only as real usage justifies them):
   - Optional rejection reasons and aggregate acceptance/edit-rate summaries.
   - Accepted-card provenance when it demonstrably helps trust or later quality analysis.
   - A TUI draft inbox or `$EDITOR` handoff if terminal-only users need approval away from desktop.
   - Desktop native reminders and rendering the stored answer-timer setting.

**Outcome:** multi-device sync feels instantaneous, iOS covers power-user daily study and quick
capture from anywhere, and humans sustain the review habit with less friction.

### Phase 3 — Strengthen the knowledge model

1. Introduce a note/template/sibling model only after defining user-facing value and migration.
2. Enforce related-card burying on top of that model.
3. Add nested decks or namespaces if flat decks limit real collections.
4. Extract a sync-provider seam only when adding a second remote backend.

**Outcome:** rem can express related cards and larger collections without compromising the simple
card/deck experience.

### Phase 4 — Add local intelligence

Start with rebuildable local embeddings rather than a scheduler or automatic author:

1. Semantic duplicate suggestions (while preserving exact duplicate outcomes as the hard rule).
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
- Supporting the browser as a reduced product while core behavior depends on Rust and local SQLite.
- Building multiple storage/sync abstractions before a second real implementation exists.

## Planning rule

Each future slice should have one user-visible outcome, a small design/spec, tests at the agreed
public seams, and an explicit release or validation step. Update this document when a phase
materially changes; keep implementation-specific investigation in dated files under
`docs/superpowers/`.
