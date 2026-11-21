# rem iOS Companion — Implementation Plan (Slice 4)

_Date: 2026-11-21 · Spec: [`docs/superpowers/specs/2026-11-21-ios-companion-design.md`](../specs/2026-11-21-ios-companion-design.md)_

This plan breaks Slice 4 (Native SwiftUI iOS Companion + shared Rust core via UniFFI) into four independently verifiable sub-slices. Each sub-slice leaves CI and all existing desktop/CLI/server tests green.

---

## Slice 4A — Rust iOS Readiness, `crates/rem-ffi`, and XCFramework Build

**Goal:** Cross-compile `rem-core` + `rem-sync` into a Swift-importable `RemCore.xcframework` with full Rust unit-test coverage of the FFI boundary.

### Tasks
1. **Prepare `rem-sync` for iOS cross-compilation** (`src-tauri/crates/rem-sync/`):
   - Make `keyring` an optional Cargo feature enabled by default (`default = ["keyring"]`) so desktop and CLI builds are unchanged while iOS omits desktop keyring crates.
   - Ensure `reqwest` uses `rustls` cleanly on `target_os = "ios"` (verify with `cargo check --target aarch64-apple-ios -p rem-sync --no-default-features`).
   - Add small helper on `rem-sync` if needed for custom redirect URI OAuth exchange (`rem://auth/callback`) using existing `generate_pkce_pair`, `generate_oauth_state`, `build_authorize_url`, and `HttpAuthClient::exchange_code`.
2. **Add Undo support to `StudySession` (or `RemStudySession`)**:
   - Store previous card state snapshot on grade so an active session can cleanly undo the immediately preceding grade and re-present that card.
   - Add pure Rust unit tests in `rem-core` verifying undo restores scheduling, counters, and queue position.
3. **Create `src-tauri/crates/rem-ffi`**:
   - Add `crates/rem-ffi` to `[workspace]` members in `src-tauri/Cargo.toml`.
   - Use `uniffi` proc-macros (`uniffi::setup_scaffolding!()`) to expose:
     - `FfiCredentialStore` callback interface bridging Swift iOS Keychain to `rem_sync::CredentialStore`.
     - `RemClient` object (decks, cards, assets, today summary, stats, backup export, OAuth start/finish, sync status, sync now, sign out, delete account).
     - `RemStudySession` object (`current_view`, `reveal`, `grade`, `undo_last_grade`, `suspend_current`, `reload_current_card`).
     - Clean flat DTO records/enums (`DeckDto`, `CardDto`, `TodaySummaryDto`, `StudyViewDto`, `StatsDto`, `SyncStatusDto`, `SyncReportDto`, `RemFfiError`).
   - Write comprehensive Rust tests inside `rem-ffi` exercising the full FFI surface against temporary SQLite DBs and an in-memory `FfiCredentialStore`.
4. **XCFramework build script (`tools/build-ios-xcframework.sh`)**:
   - Builds `rem-ffi` for `aarch64-apple-ios` and `aarch64-apple-ios-sim`.
   - Runs `uniffi-bindgen generate --library ... --language swift` to emit Swift bindings + C header + modulemap.
   - Packages `ios/Frameworks/RemCore.xcframework` via `xcodebuild -create-xcframework`.

### Verification Gate (4A)
- `(cd src-tauri && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace)` passes.
- `bash tools/build-ios-xcframework.sh` succeeds and produces `ios/Frameworks/RemCore.xcframework` + Swift bindings.

---

## Slice 4B — SwiftUI App Shell, Quiet Paper Theme, Today, Decks & Quick Capture

**Goal:** Runnable iOS Simulator app with authentic Quiet Paper styling, real SQLite persistence via `RemCore.xcframework`, 4 tabs + bottom center `+` Quick Capture, and full deck/card management.

### Tasks
1. **Create iOS project structure under `ios/`**:
   - `ios/Rem.xcodeproj` (iOS 17.0+, Bundle ID `com.shettyh.rem`, URL scheme `rem` registered in `Info.plist`).
   - `RemCore.xcframework` linked into `RemApp` + `RemAppTests`.
2. **Quiet Paper Design System (`ios/RemApp/Theme/`)**:
   - `RemTokens.swift`: Exact Light/Dark color palette matching `src/ui/tokens.css`, typography helpers, spacing & corner radii, `CardTextSize` (`compact`, `default`, `large`).
   - Reusable primitives: `RemCard`, `RemPrimaryButton`, `RemSecondaryButton`, `SyncStatusPill`, `StatusBadge`.
3. **App State & Actor Isolation (`ios/RemApp/App/`)**:
   - `RemStore` (background Swift actor wrapping `RemClient` over `Application Support/rem/rem.sqlite`).
   - `AppState` (`@Observable @MainActor`) holding decks, today summary, sync status, preferences, and toast messages.
   - `RootTabView`: 4 tabs (`Today`, `Decks`, `Stats`, `Settings`) + dedicated bottom center `+` button launching `QuickCaptureSheet`.
4. **Core Screens**:
   - `FirstRunView` (zero login wall: Connect rem Cloud vs. Start locally vs. Self-hosted URL).
   - `TodayView` (greeting, sync pill, pull-to-refresh, due hero card, All Caught Up state, deck list).
   - `DecksView` + `NewDeckSheet` (deck search, deck creation with rem color palette).
   - `DeckDetailView` (card search, tag filter chips, image badges, FSRS status pills, `Study (N)` button).
   - `QuickCaptureSheet` (Write/Preview toggle, deck selector, Markdown keyboard-accessory toolbar, `PhotosPicker` → SHA-256 `put_asset`, tag editor; supports both New Card and Edit Card).

### Verification Gate (4B)
- `xcodebuild build` and `xcodebuild test` pass on iOS Simulator.
- Manual Simulator smoke check: create deck + card (with photo), force-quit app, relaunch → data persists in SQLite.

---

## Slice 4C — Focused Review, Rich Markdown/Code & Offline Media

**Goal:** Deliver the signature `rem` daily recall experience on iPhone with rich Markdown, dark syntax-highlighted code blocks, offline image lightbox, haptics, and in-session actions.

### Tasks
1. **Rich Markdown & Code Renderer (`ios/RemApp/Markdown/`)**:
   - Parse card front/back Markdown (headings, bold/italic, lists, inline code chips, blockquotes, links, fenced code blocks, and `![](asset:<hash>)` images).
   - `CodeBlockView`: Always-dark `#1E1E22`/`#0E0C0B` rounded block with horizontal scroll and clean lightweight syntax highlighting (keywords, strings, comments, types, numbers).
   - `AssetImageView`: Loads bytes from `RemClient::get_asset(hash)` via an in-memory `NSCache` + disk cache, supports animated GIFs, graceful offline placeholder if missing, and tap-to-open `ImageLightboxView` (pinch-to-zoom + double-tap reset).
2. **Focused `ReviewView` (`.fullScreenCover`)**:
   - Zero-tab header (`1 / N`, deck name, `⋯` button, `×` close, 2pt purple progress hairline).
   - Tap-anywhere + `Show answer` reveal with `180ms` transition.
   - 4-button FSRS-6 bottom dock (`Again / Hard / Good / Easy`) wired directly to `RemStudySession::grade` with live intervals and optional haptic feedback (`UIImpactFeedbackGenerator`).
   - `ReviewMenuSheet` (`⋯`): Edit card (pre-filled `QuickCaptureSheet` → `reload_current_card`), Undo last grade, inline S/M/L text size, Suspend card.
   - `SessionCompleteView`: Recall summary, sync confirmation, `Back to Today`.

### Verification Gate (4C)
- Unit tests for Markdown/code block/asset extraction and `ReviewViewModel` state transitions (reveal, grade, undo, suspend, complete).
- Simulator visual check in Light & Dark modes across S/M/L text sizes, long code snippets, and zoomed image diagrams.

---

## Slice 4D — Stats, Settings, Cloud OAuth & End-to-End Sync

**Goal:** Complete Stats, Settings, native Apple/Google Cloud Sync via `ASWebAuthenticationSession` + iOS Keychain, daily reminders, JSON backup export, and verify desktop ↔ iPhone convergence.

### Tasks
1. **Stats Screen (`ios/RemApp/Features/Stats/`)**:
   - `7D / 30D / 90D` range selector + optional deck scope.
   - 2×2 KPI grid, purple daily activity bar chart, grade distribution bars, per-deck breakdown backed by `RemClient::get_stats`.
2. **iOS Keychain & `ASWebAuthenticationSession` (`ios/RemApp/Features/Settings/`)**:
   - `KeychainStore.swift` implementing `FfiCredentialStore` via `Security.framework` (`kSecClassGenericPassword`, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`).
   - `AuthSessionManager.swift` driving `ASWebAuthenticationSession` for Apple & Google OAuth with `rem://auth/callback`, PKCE verification, and `AccountMismatch` confirmation dialog.
3. **Settings Screen**:
   - rem Cloud status card (`Synced`, `Offline · N queued`, `Sync error · Retry`), `Sync now`, collapsible `Server URL` for self-hosted Workers, `Sign out`, and `Delete cloud account`.
   - Reading & Habit controls: Card text size (`S / M / L`), Theme (`System / Light / Dark`), Haptics toggle, local Daily Reminder via `UNUserNotificationCenter` (scheduled conditionally when due > 0).
   - Local Storage: DB & cached asset summary + `Export JSON backup` via SwiftUI `ShareLink`.
4. **Automatic Sync Lifecycle**:
   - Foreground sync (debounced), post-review sync, post-capture sync, and offline graceful degradation.

### Verification Gate (4D)
- `xcodebuild test` passes on iOS Simulator (including `KeychainStore`, `StatsViewModel`, and offline sync error handling tests).
- End-to-end convergence test against live/local `sync-server`:
  1. Connect iPhone Simulator + Desktop app to the same account.
  2. Create cards with images on Desktop → pull on iPhone → images render offline.
  3. Grade cards + add a new card offline on iPhone → reconnect → Desktop and `rem` CLI reflect exact FSRS schedules and new card with zero conflicts.
  4. Force-quit and reopen iPhone app mid-queue → state and Keychain credentials survive cleanly.
