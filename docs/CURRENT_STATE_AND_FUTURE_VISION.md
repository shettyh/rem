# rem — Current State & Future Vision

_Last reviewed: 2026-11-24 · based on `v0.25.3` plus `rem-core` undo rollback hardening and `sync-server` maintenance/deploy automation_

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
  choices, custom study, leeches, review persistence, atomic undo (restoring card state, removing
  the undone review log and daily-cap increment, and retracting unsent sync outbox ops), counters,
  requeueing, and stale-card conflicts.
- The desktop review page, `rem study` TUI, and iOS `ReviewScreen` are all thin adapters over the
  same `StudySession` implementation.
- `rem study` reviews all decks; `rem study --deck <id-or-exact-name>` reviews one deck. The
  interactive TUI supports Space/Enter reveal, 1–4 grading, arrow/j/k scrolling, resize, clean `q`
  exit, interval previews, progress, completion counts, leech notices, stale-card messages, and
  explicit image/GIF placeholders containing asset hashes.
- Custom desktop study modes include study ahead, additional new cards, forgotten cards, and
  non-rescheduling new-card preview.
- Per-deck controls cover desired retention, learning/relearning steps, new/review limits, insertion
  order, minimum/maximum intervals, leech behavior, and personalized FSRS weights on both desktop
  and iOS.

### iOS companion app

- Native SwiftUI iPhone app (`ios/RemApp/`, iOS 17+) following the Quiet Paper Light/Dark visual
  system using native iOS `TabView`, `Form`, `List`, toolbars, Swift Charts, and a `UIScrollView`
  pinch-to-zoom lightbox.
- Navigation uses a 4-tab native `TabView` (`Today`, `Decks`, `Stats`, `Settings`) with top-right
  toolbar `+` Quick Capture and inline Today sync status, hiding chrome during full-screen review.
- **First-run & local-first:** No startup login wall — users can start locally on device SQLite,
  connect to `rem Cloud`, or configure a self-hosted sync server URL.
- **Focused review:** Tap-to-reveal + 4-button FSRS-7 grade dock with live intervals, optional
  haptics, adjustable card text size (`S / M / L`), in-review card editing, undo last grade, card
  suspension, card info sheet, dark syntax-highlighted code blocks, and offline `asset:<hash>`
  image/GIF rendering with a pinch-to-zoom lightbox.
- **Quick capture, deck browsing & FSRS options:** Fast card creation/editing with Write/Preview
  toggle, Markdown keyboard toolbar, tag editor, iOS Photos image attachment (SHA-256 stored in
  SQLite), deck/tag filtering, and full deck editing + FSRS deck options & weight optimization.
- **Stats, habits, billing & App Store readiness:** 7D/30D/90D Swift Charts Stats dashboard with
  per-deck breakdown, native Sign in with Apple (`ASAuthorizationController` + token revocation on
  account delete), Google OAuth + iOS Keychain, StoreKit 2 IAP (`com.shettyh.rem.premium.yearly`),
  Settings Account drill-down, Privacy Manifest (`PrivacyInfo.xcprivacy`), local daily reminders via
  `UNUserNotificationCenter`, and JSON backup export via the iOS share sheet.

### Insight and history

- Review history records FSRS-effective grades.
- Stats dashboards (desktop and iOS) support global or per-deck scope, 7/30/90-day ranges, review
  activity, recall rate, streak, active days, grade distribution, and deck breakdown.
- Daily-cap counters are local to each device; review history is included in backup and sync.

### Data, backup, sync, and billing

- Native desktop, CLI, and iOS operations use SQLite through `rem-core`; the TypeScript `Storage`
  interface keeps React independent of native commands, and Dexie remains a browser-test adapter.
- SQLite schema v4 contains FSRS-7 scheduling state, local-only drafts, per-card optimistic
  `local_revision` values, per-domain Hybrid Logical Clocks (HLCs), and local `sync_meta` /
  `sync_outbox` / `synced_assets` bookkeeping tables.
- Deck-scoped JSON export/import preserves scheduling and review history, with confirmation before a
  same-named deck is replaced (import is desktop-only today; export is supported on desktop and iOS).
- Legacy Git sync has been removed in favor of optional Cloud sync (`Local only` vs. `Cloud sync`).
  The shared `rem-sync` crate talks to the open-source Cloudflare Workers + SQLite Durable Objects +
  D1 + R2 backend in `sync-server/` (either hosted `rem Cloud` or a self-hosted deployment).
- Hosted `rem Premium` implements a structured entitlement state machine (`GET /v1/account`), an
  opt-in one-time 30-day no-card free trial (`POST /v1/trial/start`), Crockford invite codes with
  D1-backed rate limiting (`POST /v1/invites/redeem` + admin CLI), Polar MoR checkout/portal/webhooks
  for Desktop & Web, Apple StoreKit 2 JWS verification + App Store Server Notifications v2 webhooks
  for iOS, and `/premium` (`/cloud`), `/privacy`, `/terms`, and `/support` web pages.
- Google OAuth and Sign in with Apple (PKCE + native iOS `ASAuthorizationController` + short-lived
  JWTs + rotating refresh tokens) store credentials in the OS Keychain; commutative per-domain HLC
  merges and tombstones resolve concurrent edits and reviews automatically without halting conflict
  dialogs.
- `sync-server` runs a daily Cloudflare Cron Trigger (`scheduled()`) to purge expired `oauth_codes`,
  expired/old revoked `refresh_tokens`, and stale rate-limit buckets, and deploys via an automated
  GitHub Actions workflow (`.github/workflows/sync-server-deploy.yml`) with remote D1 migrations.
- Unapproved drafts and local daily-cap counters are intentionally excluded from backup and cloud sync.
- Manual sync from Settings, automatic sync triggers (startup/foreground, debounced writes,
  focus/online, periodic, post-review), account deletion (including Apple token revocation and
  best-effort Polar cancellation), and `rem sync` CLI commands are implemented.

### Desktop experience and delivery

- Collapsible icon-rail sidebar and native content shell with light and dark themes, dense cards
  list, custom icons, and native window chrome.
- Installers build for Apple Silicon and Intel macOS (Developer ID signed & notarized), Linux, and Windows.
- The `rem` CLI binary is bundled directly inside desktop packages (`rem.app/Contents/MacOS/rem` on macOS) so Homebrew (`brew install --cask shettyh/tap/rem`) and `install.sh` provide a single installation that stays updated in lockstep with the desktop app; standalone CLI archives are also published on GitHub Releases.
- Signed in-app auto-updates (`tauri-plugin-updater`) check `latest.json` on GitHub Releases and update both desktop and linked CLI together.
- CI covers TypeScript/unit tests, real-browser flows, `sync-server` Workers tests, Rust formatting,
  Clippy, Rust workspace tests, frontend builds, iOS `RemCore.xcframework` + `RemAppTests` Simulator
  tests, security auditing, cross-platform desktop/CLI release builds, automated `sync-server`
  deploys, and cloud-signed iOS TestFlight uploads.

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

1. **`rem-core` sync engine** — SQLite HLCs, 7 commutative op types, transactional `sync_outbox`,
   `apply_remote_ops`, and bootstrap generator.
2. **Cloudflare `sync-server` + `rem-sync`** — Worker, per-user SQLite `UserSyncDO`, R2 content-addressed
   asset routes, D1 auth/entitlements, and Rust `rem-sync` client.
3. **Auth, Desktop UI, CLI sync & Git sync retirement** — Apple + Google OAuth, OS Keychain, hosted
   `rem Cloud` default + self-hosted server URL, `rem sync` CLI, and clean removal of legacy Git sync.
4. **Native SwiftUI iOS Companion (v1)** — `rem-ffi` UniFFI crate, `RemCore.xcframework` build script,
   and full SwiftUI iPhone app covering onboarding, Today, Decks, Review, Quick Capture, Stats,
   Settings, Cloud OAuth, local reminders, and backup export.

### 3. Distribution, `rem Premium` Billing & iOS Native App Store Readiness (`v0.19.0`–`v0.25.3`)

1. **Single-install distribution & FSRS-7** — FSRS-7 migration (SQLite schema v4), signed & notarized
   macOS releases, bundled `rem` CLI, signed Tauri auto-updater, and automated Homebrew cask tap.
2. **`rem Premium` entitlements & billing** — unified entitlement state machine, optional 30-day
   no-card trial, Crockford invite codes + rate limiting, Polar checkout/portal/webhooks, Apple
   StoreKit 2 IAP + ASN v2 webhooks, and `/premium`, `/privacy`, `/terms`, `/support` web pages.
3. **iOS native polish & App Store compliance** — native `TabView`, `Form`/`List` surfaces, Swift
   Charts, Study screen & Settings Account drill-down redesigns, full FSRS Deck Options + optimizer,
   native Sign in with Apple + revocation, Privacy Manifest, iOS Simulator CI, and automated
   TestFlight uploads.

## Known gaps and incomplete behavior

These are real gaps in the resulting product, not features already delivered.

| Area | Current limitation |
|---|---|
| Desktop & TUI study parity | `StudySession::undo_last_grade()` and `suspend_current()` exist in `rem-core` and are wired on iOS, but Desktop (`ReviewPage`) and `rem study` TUI do not expose Undo (`⌘Z`/`u`) or in-review Suspend yet. |
| iOS study & backup parity | Custom Study modes (already in `rem-core`) and JSON backup import are desktop-only; iOS currently supports regular study and backup export only. |
| Cross-deck search | Card search is scoped to a single deck on both Desktop and iOS; there is no global search across all decks. |
| Cloud sync v1.1 | `UserSyncDO` keeps an unbounded append-only `ops` log (new devices replay from `seq = 0` in 500-op batches) and clients rely on focus/polling triggers rather than compacted `/v1/bootstrap` snapshots and real-time WebSocket pokes. |
| iOS habit & capture extensions | WidgetKit Home/Lock Screen widgets (requiring App Group SQLite migration) and a Safari/system Share Sheet capture extension are not yet built. |
| Draft feedback | Rejection reasons, acceptance/edit-rate summaries, and agent-readable quality feedback are not recorded. |
| Draft provenance | Rationale/source/producer metadata is not retained on the accepted card. |
| Duplicate help | Draft/card duplicate checks are exact normalized matches, not semantic suggestions. |
| Draft portability | Pending drafts are local-only and cannot be triaged from another machine or the TUI. |
| Habit support | No daily reminders or native notifications on desktop (iOS supports local daily reminders via `UNUserNotificationCenter`). |
| Review options | **Show answer timer** is stored in `DeckSettings` but not rendered. |
| Related-card burying | `bury_related` is stored in `DeckSettings` but cannot be enforced without a note/template/sibling model. |
| Organization | Decks are flat; namespaces, nested decks, and folders do not exist. |
| CI & Distribution | Windows installers and CLI binaries do not yet have an Authenticode signature; public App Store 1.0 release (beyond TestFlight) is not yet submitted. |

## Future direction

### Phase 1 — App Store Launch & Production Validation

1. Submit the iOS 1.0 companion build for public App Store release (building on the TestFlight
   and App Store compliance work shipped in `v0.22–v0.25.3`).
2. End-to-end smoke-test live production flows across all surfaces: Polar checkout ↔ iOS unlock,
   StoreKit 2 purchase ↔ Desktop unlock, external agent draft → desktop acceptance → desktop/TUI
   study, and physical iPhone ↔ desktop/CLI offline sync + photo/asset convergence.
3. Configure `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_D1_DATABASE_ID`, and
   `SYNC_SERVER_HEALTH_URL` in GitHub Actions so `sync-server` migrations and deploys stay in lockstep
   with `main`.

**Outcome:** `rem` is publicly available on Desktop, CLI, and the iOS App Store, with trustworthy
managed Cloud sync and repeatable production operations.

### Phase 2 — Cross-Surface Study Parity (Small Rust Lift, High Daily UX Payoff)

Close the highest-friction mismatches between Desktop, iOS, and `rem study` by surfacing capabilities
already built in `rem-core`:

1. **Undo (`⌘Z`/`u`) & Suspend in Desktop and `rem study` TUI:** wire `StudySession::undo_last_grade()`
   and `suspend_current()` through `src-tauri/src/study.rs`, Desktop `ReviewPage`, and `rem-cli`'s
   `tui.rs` so recovering from a mis-tapped grade works identically everywhere.
2. **Custom Study & JSON Backup Import on iOS:** expose `CustomStudyRequest` (`studyAhead`,
   `increaseNew`, `reviewForgotten`, `previewNew`) and backup import through `rem-ffi` and iOS
   Deck/Settings UI.
3. **Desktop habit polish** (as usage justifies): optional desktop native daily reminders and
   rendering the stored answer-timer setting.

*(Maintainability note when touching Settings/CLI: decompose `src/features/settings/SettingsPage.tsx`,
`ios/RemApp/Features/Settings/SettingsView.swift`, and `src-tauri/crates/rem-cli/src/main.rs` into
focused submodules as new controls are added.)*

**Outcome:** studying, recovering from mistakes, cramming/previewing, and restoring backups feel
equally complete on desktop, terminal, and iPhone.

### Phase 3 — Cloud Sync v1.1 (Instant Sync & Bounded Startup)

1. **Server-side op compaction & `/v1/bootstrap`:** periodically compact `UserSyncDO`'s `ops` log
   into gzipped R2 state snapshots so new devices and long-lived accounts with thousands of reviews
   bootstrap in a single download rather than replaying every historical op from `seq = 0`.
2. **Real-time Hibernatable WebSocket pokes:** add a lightweight Durable Object WebSocket channel
   that sends a small "new seq available" poke to connected devices so Desktop ↔ iPhone updates feel
   instantaneous without continuous polling.
3. **Optional non-Cloudflare self-host server** only if self-hosters demonstrate demand for a
   VPS/Docker deployment target.

**Outcome:** multi-device sync feels instantaneous and stays fast regardless of collection age.

### Phase 4 — iOS Habit & Capture Multipliers

1. **App Group SQLite migration:** move the iOS SQLite collection into a shared App Group container
   with safe WAL coordination between the main app and extensions.
2. **WidgetKit Home & Lock Screen widgets:** glanceable due counts and streak indicators that tap
   directly into Today or Review.
3. **Safari / system Share Sheet capture extension:** capture quotes, snippets, or URLs from Safari,
   Notes, or readers into a chosen deck in two taps.

**Outcome:** iOS becomes a zero-friction capture inbox and daily habit anchor, not just a review client.

### Phase 5 — Global Cross-Deck Search & Knowledge Organization

1. **Global cross-deck search:** add a fast `search_cards` query (backed by SQLite FTS5 or indexed
   search in `rem-core`) powering `⌘K` quick-find on Desktop and global search on iOS.
2. **Draft workflow refinements** (as real usage justifies): optional rejection reasons,
   acceptance/edit-rate summaries, retained provenance on accepted cards, and a TUI draft inbox or
   `$EDITOR` handoff for terminal-first users.
3. **Knowledge model evolution** (only after clear user-facing value and migration design):
   - Introduce a note/template/sibling model and enforce `bury_related` on top of it.
   - Add nested decks or namespaces if flat decks limit real collections.

**Outcome:** large multi-deck collections stay easy to search, organize, and navigate without losing
the calm card/deck experience.

### Phase 6 — Local Intelligence & Assisted Source-to-Draft Authoring

Start with rebuildable local embeddings rather than a scheduler or automatic author:

1. Semantic duplicate suggestions (while preserving exact duplicate outcomes as the hard rule).
2. Semantic search and related-card suggestions during review.
3. Topic-aware interleaving experiments.
4. If users need generation inside rem beyond external agents + CLI, add a small `CardGenerator`
   boundary that imports selected text/files into the existing try/edit/accept/reject Drafts inbox
   without silently uploading deck content. An MCP adapter is justified only when a real client
   cannot invoke the local CLI reliably.

**Outcome:** rem surfaces overlap and connections locally and turns source material into recall
prompts faster without weakening human control.

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
