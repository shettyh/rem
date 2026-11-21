# rem iOS Companion — Design Specification

_Date: 2026-11-21 · Status: approved design for Slice 4 implementation_
_Parent architecture: [`2026-11-21-cloudflare-cloud-sync-and-mobile-design.md`](2026-11-21-cloudflare-cloud-sync-and-mobile-design.md)_

---

## 1. Summary & Goals

Build a focused, native **iPhone companion app** for `rem` using **SwiftUI** backed by the existing shared Rust **`rem-core`** and **`rem-sync`** crates via **UniFFI** (`crates/rem-ffi`).

Desktop + CLI remain the primary surfaces for deep authoring and AI-agent draft workflows; iPhone is optimized for **calm, fast, offline-first daily recall** and **under-10-second card capture**.

### Core Product Decisions (Locked)

1. **Visual direction — Quiet Paper:** Faithful mobile sibling of `rem` desktop ([`src/ui/tokens.css`](../../../src/ui/tokens.css)) — warm stone neutrals, hairline borders, generous reading typography, dark syntax-highlighted code blocks in both themes, and single-accent purple (`#5a47e8` light / `#8174ff` dark) reserved strictly for primary actions, progress, and Easy grade.
2. **Navigation — 4 tabs + dedicated bottom center `+` button:** `Today`, `Decks`, `[+]` (Quick Capture), `Stats`, `Settings`. No redundant top-right `+` buttons on Today, Decks, or Deck Browser.
3. **Review interaction — Tap-to-reveal + 4-button FSRS dock:** Focused full-screen review hides the tab bar completely. Tapping anywhere on the card (or `Show answer`) reveals the answer; grading uses the 4-button bottom dock (`Again / Hard / Good / Easy`) with live next-review intervals from Rust `StudySession`. No accidental swipe-to-grade gestures.
4. **Mobile capture — Normal due cards:** Quick Capture creates normal due cards directly in SQLite (enqueued into `sync_outbox` when Cloud Sync is enabled). Unapproved agent `card_drafts` remain strictly local to desktop + CLI.
5. **Implementation stack — Native SwiftUI + Rust UniFFI (`crates/rem-ffi`):** Compiles `rem-core` and `rem-sync` into `RemCore.xcframework` (`aarch64-apple-ios` + `aarch64-apple-ios-sim`), preserving one SQLite schema, one FSRS-6 `StudySession`, and one Cloud Sync engine across macOS, CLI, and iPhone.

---

## 2. Visual System (Quiet Paper Tokens)

All SwiftUI colors live in a single `RemTheme` asset/token table matching desktop CSS tokens:

| Role | Light | Dark | Usage |
| --- | --- | --- | --- |
| `bg` | `#FFFFFF` | `#161413` | Primary screen canvas |
| `bgInset` | `#F5F4F2` | `#201E1C` | Search fields, subtle pills, hover/press wash |
| `sidebar` / `chrome` | `#FAFAF9` | `#0E0C0B` | Keyboard accessory bar, diagram card canvas |
| `surface` | `#FFFFFF` | `#1B1918` | Elevated grouped cards, list containers |
| `surfaceInset` | `#F1F0EE` | `#24211F` | Inline code chips, muted badges |
| `border` | `#ECEAE7` | `#2B2825` | Hairline 1px/0.5pt structural dividers |
| `borderStrong` | `#D9D6D2` | `#3C3835` | Secondary buttons, blockquote rule |
| `text` | `#1C1917` | `#FAFAF9` | Primary text |
| `muted` | `#6F6965` | `#B0AAA5` | Secondary copy, timestamps, counts |
| `accent` | `#5A47E8` | `#8174FF` | Primary CTA, active tab, review progress, activity bars, Easy grade |
| `codeBg` / `codeFg` | `#1E1E22` / `#ECEAF6` | `#0E0C0B` / `#ECEAF6` | Fenced code blocks (always dark) |
| `again` / `hard` / `good` / `easy` | `#DC2A2F` / `#D97914` / `#1E9B5E` / `#5A47E8` | `#E5484D` / `#E8922E` / `#2FA86B` / `#8174FF` | FSRS-6 grade labels & distribution bars |

### Typography & Card Reading Scale
- **UI & Body:** System SF Pro (`Display` for greeting/KPI numbers with tight tracking, `Text` for body & lists).
- **Code & Intervals:** Monospace (`SF Mono` / `Space Mono` where bundled) for inline code, fenced code blocks, review counters (`1 / 3`), and FSRS interval labels (`10m`, `1d`, `3d`, `7d`).
- **Adjustable Card Text Size (`S / M / L`):** Stored in `UserDefaults`, live-adjustable from both Settings and the in-review `⋯` sheet:
  - `compact`: Question `18pt` (`14.5pt` revealed), Answer `14.5pt`, Code `11pt`
  - `default`: Question `21pt` (`16.5pt` revealed), Answer `16pt`, Code `12pt`
  - `large`: Question `24pt` (`18pt` revealed), Answer `18pt`, Code `13pt`

---

## 3. Navigation & Screen Architecture (v1)

### Bottom Navigation Bar
Custom SwiftUI translucent floating/docked bar with 5 equal slots:
1. **Today** — calendar icon
2. **Decks** — stacked cards icon
3. **`[+]` Quick Capture** — dark high-contrast rounded square (`var(--text)` fill, `var(--bg)` plus glyph); opens `QuickCaptureSheet` modal from any tab
4. **Stats** — bar chart icon
5. **Settings** — gear icon

Hidden completely when presenting `ReviewView` (`.fullScreenCover`).

### v1 Screen Inventory

1. **First-Run Empty State** (when local DB has 0 decks and sync is not configured)
   - Zero login wall: explains on-device SQLite privacy and optional Cloud Sync.
   - Primary CTA: **Connect rem Cloud** (opens Apple/Google sign-in flow).
   - Secondary CTA: **Start locally without an account** (enters Today/Decks immediately).
   - Tertiary link: **Using a self-hosted server?** (jumps to Settings with custom URL expanded).

2. **Today**
   - Top row: formatted date + subtle **Sync Status Pill** (`Synced · 2m ago`, `Local only`, `Offline · N queued`, or `Sync error · Retry`). Pull-to-refresh triggers a background sync.
   - Greeting (`Good morning / afternoon / evening.`) + due summary subtitle.
   - **Due Hero Card:** Large due number, new/review breakdown, estimated duration (`~N min`), subtle streak pill, and full-width purple `Start review →` button.
   - **All Caught Up State (0 due):** Calm check badge, today's completed count + recall %, decks remain browsable below.
   - **Your Decks List:** Deck color dot, name, subtle mastery progress bar, due/new counts; tapping opens `DeckDetailView`.

3. **Decks Tab + New Deck Sheet**
   - Large title `Decks`, search bar across decks/cards, full deck list with card counts + FSRS target retention, and `+ Create new deck` button.
   - **New Deck Sheet:** Deck name field + 6 quiet rem color dots + Create action.

4. **Deck Browser (`DeckDetailView`)**
   - Top bar: back to Decks + compact `Study (N)` button.
   - Deck heading + card/due summary, search input, horizontal **tag filter chips** (`All`, `#tag…`).
   - Card rows showing front text preview, tag chip, image attachment indicator (when `asset:<hash>` refs exist), and FSRS status pill (`Due`, `Learning · 10m`, `14d`, `Suspended`). Tapping a row opens the card in Edit mode.

5. **Focused Review (`ReviewView`)**
   - Full-screen presentation (no tab bar).
   - **Top Bar:** `current / total` counter, deck name, `⋯` action menu button, `×` close button, and a 2pt purple progress bar along the bottom edge.
   - **Reading Canvas (scrollable):**
     - `QUESTION` label + rendered Markdown prompt. Tapping anywhere reveals the answer when unrevealed.
     - On reveal (`180ms` ease): prompt softens slightly to muted secondary scale, hairline divider appears, `ANSWER` renders Markdown with inline code chips, horizontally scrollable dark syntax-highlighted code blocks, blockquotes, and offline-cached `asset:<hash>` images/GIFs.
     - **Image Lightbox:** Tapping any rendered image/diagram opens a full-screen dark zoomable viewer (pinch-to-zoom, double-tap reset).
   - **Bottom Thumb Dock:**
     - Unrevealed: full-width purple `Show answer` button.
     - Revealed: 4 equal FSRS-6 grade buttons (`Again`, `Hard`, `Good`, `Easy`) with semantic colors and live interval previews (`10m`, `1d`, `3d`, `7d`). Triggers optional light haptic impact on reveal and medium selection haptic on grade.
   - **In-Review `⋯` Action Sheet:**
     - **Edit card** — opens `QuickCaptureSheet` pre-populated with current card; saves via `Collection::update_card` (`CardContentPatch`) without resetting scheduling.
     - **Undo last grade** — restores the previous review step if graded in the current session (reversing the local grade before next sync or recording a clean undo snapshot — see Section 5).
     - **Card text size** — inline `S / M / L` segmented toggle.
     - **Suspend card** — destructive action, suspends card (`CardSuspendPatch`) and advances queue with toast.
   - **Session Complete State:** Calm summary (cards reviewed, recall %, local/cloud sync confirmation) + `Back to Today`.

6. **Quick Capture Sheet (`QuickCaptureSheet`)**
   - Modal sheet (`Cancel`, `New card` / `Edit card`, `Save`).
   - Deck picker pill (defaults to currently viewed deck or last-used deck) + `Write / Preview` segmented toggle.
   - Question & Answer Markdown editors + tag chip editor.
   - **Photo/Image Picker:** Attaches image from iOS Photos, hashes bytes (SHA-256), saves to `rem-core` `assets` table, and inserts `![](asset:<sha256>)` in Markdown.
   - **Keyboard Accessory Toolbar:** One-tap `B`, `I`, `` `code` ``, ` ``` `, `Photo`, `#tag`.

7. **Stats Tab**
   - `7D / 30D / 90D` segmented switcher (+ deck filter).
   - 2×2 KPI grid (`Reviews`, `Recall rate`, `Current streak`, `Active days`).
   - Daily review activity bar chart (purple bars, Swift Charts or native geometry bars).
   - Grade distribution horizontal bars (`Again / Hard / Good / Easy`) + per-deck recall breakdown.

8. **Settings & Cloud Sync Tab**
   - **Cloud Sync section:**
     - *Signed in:* Provider + email, last synced time / offline pending outbox count / error banner, `Sync now`, collapsible `Server URL` (defaults to `rem Cloud`, configurable to any self-hosted Worker URL), `Sign out (keep local cards)`, and `Delete cloud account` (behind confirmation alert).
     - *Local-only:* Explanation + `Continue with Apple` and `Continue with Google` buttons opening `ASWebAuthenticationSession`.
   - **Reading & Habit section:** Card text size (`S / M / L`), Theme (`System / Light / Dark`), Haptic grade feedback toggle, Daily Reminder time (local `UNUserNotificationCenter` notification scheduled only if due count > 0).
   - **Local Storage section:** SQLite card count + cached asset size, `Export JSON backup` (iOS `ShareLink` / system share sheet).

---

## 4. Technical Architecture: SwiftUI + Rust (`rem-ffi`)

```text
ios/
├── Rem.xcodeproj                  # iOS app target (iOS 17+ deployment target)
├── RemApp/                        # SwiftUI app entry, AppState, Views, Theme, Markdown
│   ├── App/                       # RemApp.swift, AppState.swift, RootTabView.swift
│   ├── Theme/                     # RemTokens.swift, CardTextSize.swift
│   ├── Features/
│   │   ├── Onboarding/            # FirstRunView.swift
│   │   ├── Today/                 # TodayView.swift
│   │   ├── Decks/                 # DecksView.swift, DeckDetailView.swift, NewDeckSheet.swift
│   │   ├── Review/                # ReviewView.swift, GradeDock.swift, ReviewMenuSheet.swift, ImageLightbox.swift
│   │   ├── Capture/               # QuickCaptureSheet.swift, MarkdownToolbar.swift
│   │   ├── Stats/                 # StatsView.swift
│   │   └── Settings/              # SettingsView.swift, AuthSessionManager.swift, KeychainStore.swift
│   └── Markdown/                  # RemMarkdownView.swift, CodeBlockView.swift, AssetImageView.swift
└── Frameworks/
    └── RemCore.xcframework        # Built by tools/build-ios-xcframework.sh

src-tauri/crates/
├── rem-core/                      # Unchanged SQLite + FSRS-6 + StudySession + SyncStore
├── rem-sync/                      # Feature-gate `keyring` + rustls TLS for iOS compatibility
└── rem-ffi/                       # UniFFI bridge exposing RemClient + RemStudySession
```

### 4.1 `rem-sync` iOS Compatibility Changes (Minimal)
1. **Optional `keyring` feature:** In `src-tauri/crates/rem-sync/Cargo.toml`, put `keyring` behind `default = ["keyring"]` so desktop/CLI behavior is 100% unchanged, while `rem-ffi` depends on `rem-sync` with `default-features = false`.
2. **Rustls TLS for iOS:** Configure `reqwest` with `rustls` (`default-features = false, features = ["blocking", "json", "rustls"]`) or target-specific iOS `rustls` so `aarch64-apple-ios` cross-compiles cleanly with zero OpenSSL/system-lib issues.
3. **`CredentialStore` trait already exists!** `rem-ffi` defines a UniFFI callback interface `FfiCredentialStore` (`load`, `save`, `delete` JSON strings) that wraps Swift's `KeychainCredentialStore` (`SecItemAdd` / `SecItemCopyMatching` / `SecItemDelete` under `com.shettyh.rem.sync`, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`) and implements `rem_sync::CredentialStore`.

### 4.2 `crates/rem-ffi` Public Surface (UniFFI)
Keep the FFI seam coarse-grained and intent-level (matching desktop Tauri commands):

- `RemClient::open(db_path: String, credential_store: Box<dyn FfiCredentialStore>) -> Result<RemClient, RemFfiError>`
- **Decks & Overview:** `list_decks()`, `get_today_summary(now_ms)`, `create_deck(name, color)`, `update_deck(id, name, color)`, `delete_deck(id)`
- **Cards & Assets:** `list_cards(deck_id)`, `create_card(deck_id, front, back, tags)`, `update_card(id, front, back, tags)`, `set_card_suspended(id, suspended)`, `delete_card(id)`, `put_asset(mime, bytes) -> hash`, `get_asset(hash) -> Option<AssetData>`
- **Study Session (`RemStudySession` object):**
  - `start_study(deck_id: Option<String>, now_ms: i64) -> Result<RemStudySession, RemFfiError>`
  - `current_view() -> StudyViewDto`
  - `reveal(now_ms: i64) -> StudyViewDto`
  - `grade(grade: GradeDto, now_ms: i64) -> StudyGradeOutcomeDto`
  - `suspend_current(now_ms: i64) -> StudyViewDto`
  - `reload_current_card() -> StudyViewDto` (after in-review edit)
- **Stats & Backup:** `get_stats(deck_id: Option<String>, days: u32, now_ms: i64) -> StatsDto`, `export_backup_json() -> String`
- **Cloud Auth & Sync:**
  - `default_cloud_server_url() -> String`
  - `fetch_auth_providers(server_url: String) -> AuthProvidersDto`
  - `begin_oauth(server_url: String, provider: OAuthProviderDto) -> OAuthStartDto` (returns `authorize_url`, `state`, `pkce_verifier`, `redirect_uri = "rem://auth/callback"`)
  - `finish_oauth(server_url: String, code: String, pkce_verifier: String, reset_existing: bool, now_ms: i64) -> ConnectResultDto`
  - `sync_now(now_ms: i64) -> SyncReportDto`
  - `sync_status() -> SyncStatusDto` (mode, signed_in, server_url, email, provider, pending_outbox_count, last_seq)
  - `sign_out(now_ms: i64)`
  - `delete_cloud_account(now_ms: i64)`

### 4.3 Native OAuth Flow (`ASWebAuthenticationSession`)
The Cloudflare `sync-server` (`sync-server/src/oauth.ts`) already whitelists `rem://auth/callback`.
1. Swift calls `client.beginOauth(serverUrl, provider)` → Rust generates PKCE verifier/challenge + state and builds `/v1/auth/{provider}/start?redirect_uri=rem://auth/callback&code_challenge=...`.
2. Swift launches `ASWebAuthenticationSession(url: authorizeURL, callbackURLScheme: "rem")`.
3. User authenticates with Apple or Google; Worker redirects to `rem://auth/callback?code=...&state=...`.
4. Swift verifies `state` matches, then calls `client.finishOauth(serverUrl, code, verifier, resetExisting, nowMs)`.
5. Rust exchanges code via `HttpAuthClient::exchange_code`, persists credentials into Swift's iOS Keychain via `FfiCredentialStore`, enables Cloud Sync (enqueuing deterministic bootstrap ops), and triggers initial `sync_now`.

### 4.4 Background Sync & Offline Resilience
- Local SQLite DB lives in `Application Support/rem/rem.sqlite` (prepared for App Group migration in v1.1 when Widgets/Share Extension arrive).
- All Rust FFI calls run off the main actor (`Task.detached` / dedicated serial actor) so UI stays locked at 60/120fps.
- Sync triggers automatically on: app foreground (throttled to ≥30s), pull-to-refresh on Today, completion of a review session, and card save when online.
- **Offline safety:** Grading, creating, editing, and suspending cards always commit locally to SQLite + `sync_outbox` first. Network failures never block study and surface only as a non-intrusive pill (`Offline · N queued` or `Sync error · Retry`).

---

## 5. Edge Cases & Explicit Scope Boundaries

### Undo Last Grade
To preserve commutative sync safety without deleting immutable `review_logs` already pushed to the server:
- In v1, **Undo last grade** is available inside an active review session *before* that card's sync op leaves the device, OR restores the card's prior FSRS scheduling snapshot locally and re-queues the card as current (emitting a newer HLC `CardReview` / scheduling restore if already synced). We will keep the exact undo snapshot in `StudySession` / `RemStudySession` and unit-test it in Rust.

### v1 vs. v1.1 Scope
- **Included in v1:** All 8 screens above, Quiet Paper Light/Dark themes, offline SQLite + FSRS-6, Cloud Sync (Apple/Google + self-hosted URL), offline image/GIF rendering + zoom, photo capture, Stats, local daily reminder, JSON backup export.
- **Deferred to v1.1:** WidgetKit Home/Lock Screen widgets, Safari Share Sheet capture extension, per-deck FSRS settings & weight optimizer UI, Custom Study modes, real-time WebSocket sync pokes.

---

## 6. Verification & Acceptance Gates

1. **Rust FFI & iOS Cross-Compile Gate:**
   - `cargo test --workspace` (including new `rem-ffi` unit tests covering study lifecycle, undo, assets, and mock `FfiCredentialStore` sync).
   - `cargo clippy --workspace --all-targets -- -D warnings`.
   - `tools/build-ios-xcframework.sh` compiles cleanly for `aarch64-apple-ios` and `aarch64-apple-ios-sim`.
2. **iOS Simulator Build & Automated Tests:**
   - `xcodebuild test` passes on iOS Simulator (ViewModel state tests, Keychain store tests, Markdown/code parser tests).
3. **End-to-End Physical / Simulator Convergence Checklist:**
   - Fresh install with zero account: create deck + card, review offline, force-quit and reopen → scheduling state persists.
   - Sign in with rem Cloud → desktop cards + images sync down cleanly.
   - Airplane mode: grade cards + capture new card with photo → shows `Offline · N queued` → reconnect → sync converges with desktop & CLI with zero conflicts.
