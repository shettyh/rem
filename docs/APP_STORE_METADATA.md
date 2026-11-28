# App Store Connect Metadata — rem (iOS)

Ready-to-paste App Store Connect listing sheet for **rem** (`com.shettyh.rem`, iOS 17.0+, iPhone).

> **Scope & Compliance Notes**
> - **Positioning:** Lead with `rem` as a standalone, calm, local-first **FSRS-7** and Anki-compatible iPhone study app where every local study feature is 100% free forever. Position the free open-source Mac/Linux/Windows desktop apps, CLI, and AI-agent capture workflow as a distinct differentiating section lower in the description (keeping the Name and Subtitle approachable and free of developer jargon).
> - **Voice:** Quiet Paper — calm, honest, focused on durable retention, zero hype or clutter.
> - **App Store Guideline 3.1.1 / 3.1.2(c) compliance:** All pricing and subscription copy references only the native App Store In-App Purchase (`com.shettyh.rem.premium.yearly`, `$4.99/year`), the in-app 30-day no-card free trial available after sign-in in Settings, or the optional free self-hosted sync server.
> - **Verified against `ios/RemApp/` (`v0.32.2`):** Every claimed capability corresponds directly to shipped SwiftUI / Rust (`rem-core` FSRS-7) code.

---

## Listing Fields & Character Limits

| Field | Limit | Actual | Status |
| --- | ---: | ---: | --- |
| App Name | 30 | 21 | Pass |
| Subtitle | 30 | 28 | Pass |
| Promotional Text | 170 | 162 | Pass |
| Keywords | 100 | 97 | Pass |
| Description | 4000 | 3392 | Pass |
| What's New (`v0.32`) | 4000 | 770 | Pass |

*(Exact character counts are verified programmatically on the copy blocks below.)*

---

### App Name (21 / 30 chars)

```text
rem — FSRS Flashcards
```

### Subtitle (28 / 30 chars)

```text
Calm Spaced Repetition Study
```

### Promotional Text (162 / 170 chars)

```text
100% free offline study with FSRS-7, direct .apkg Anki deck import, Cloze, Image Occlusion, LaTeX math, audio & Swift Charts stats. Optional real-time Cloud Sync.
```

### Keywords (97 / 100 chars)

*Comma-separated, no spaces after commas, no words repeated from App Name or Subtitle.*

```text
apkg,srs,cloze,occlusion,latex,markdown,offline,decks,memory,exam,mcat,medical,audio,recall,notes
```

### Primary & Secondary Categories

- **Primary Category:** Education
- **Secondary Category:** Productivity
- **Age Rating:** 4+

---

### Description (3392 / 4000 chars)

```text
Calm, local-first spaced repetition for iPhone powered by FSRS-7. Study completely offline with no account required—every local study feature is 100% free, forever.

BRING YOUR EXISTING DECKS
Import Anki .apkg and .colpkg archives directly on your iPhone (up to 250 MB) without touching a computer. Your cards, review history, embedded images, audio clips, and nested :: subdecks transfer cleanly so you never lose your study progress.

BUILT FOR SERIOUS STUDYING
• FSRS-7 Adaptive Scheduling — Tune desired retention (70%–99%), learning and relearning steps, daily new-card and review caps, sibling burying, and leech handling per deck, or run one-tap personalized FSRS weight optimization from your own review history.
• Rich Card Formats — Study with Cloze deletions and hints, interactive Image Occlusion (draw and edit rectangle or ellipse masks right on iPhone with Hide All / Hide One modes and a zoomable lightbox), native inline and block LaTeX math, Markdown tables, syntax-highlighted code blocks, Ruby/furigana readings, and inline audio playback.
• Nested Subdecks & Fast Search — Organize large subjects with collapsible subdeck hierarchies and rolled-up due counts, then find any deck, card, tag, or rationale instantly with full-text search.
• Custom Study — Study ahead for an exam, raise today's new-card limit, drill recently forgotten cards, or preview newly added cards without changing schedules.
• Native Swift Charts Statistics — Scrub across 7, 30, or 90 days of daily review activity, and track recall rate, streaks, grade distribution, and per-deck breakdowns.
• Quiet Paper Focus — Light and dark themes, adjustable card text sizes, haptic grade feedback, undo last grade, and a smart daily reminder that only notifies you when cards are still due.

OPTIONAL REAL-TIME CLOUD SYNC
Every local feature stays free forever. When you want your decks, review history, and media (images, diagrams, and audio) synced in real time across devices:
• Sign in with Apple or Google in Settings and start a 30-day free trial with no payment method required.
• Subscribe to rem Premium ($4.99/year via the App Store) for real-time Cloud Sync with instant foreground updates and 500 MB of isolated per-account media storage.
• Prefer full infrastructure ownership? Connect to your own self-hosted Apache-2.0 sync server directly from Settings.

DESKTOP, CLI & AI-AGENT ECOSYSTEM
Pair your iPhone with the free, open-source rem desktop apps for Mac, Linux, and Windows—plus a native CLI and open agent skill for capturing source-grounded flashcards from coding sessions, technical docs, and research papers. Cards keep their source references and rationale right inside the iPhone review screen so you always remember why a concept matters.

PRIVATE BY DEFAULT
No ads, no tracking, and no lock-in. Your collection lives in a local SQLite database on your iPhone, with full JSON backup export and import anytime.

SUBSCRIPTION DETAILS
rem Premium Annual ($4.99/year) is an optional auto-renewable subscription for managed Cloud Sync. Payment is charged to your Apple Account at confirmation of purchase. Subscription automatically renews annually unless canceled at least 24 hours before the end of the current period. Manage or cancel anytime in your App Store Account Settings.

Privacy Policy: https://rem-sync.shettyh.fyi/privacy
Terms of Use: https://rem-sync.shettyh.fyi/terms
```

---

### What's New (`v0.32`, 770 / 4000 chars)

```text
What's new in rem 0.32:

• Image Occlusion on iPhone — Review and create Image Occlusion cards right on your iPhone with rectangle and ellipse masks, Hide All / Hide One modes, zoomable lightbox inspection, and full import support for Anki 23.10+ and Image Occlusion Enhanced .apkg decks.
• Real-Time Cloud Sync — Instant foreground sync updates via WebSocket pokes with zero background battery drain, plus faster initial sync with compacted snapshots.
• Global Full-Text Search — Search across all decks, cards, tags, and card rationales directly from the Decks tab.
• Source & Rationale Provenance — Inspect retained source references and card rationale right inside the review screen and Card Info sheet.
• Native Sign in with Apple & polished Quiet Paper onboarding.
```

---

## URLs & App Store Connect Links

All URLs below are verified directly from `ios/RemApp/Theme/RemTokens.swift` (`RemDocs`) and the repository website configuration:

| App Store Connect Field | URL | Source in Repo |
| --- | --- | --- |
| Marketing URL | `https://shettyh.github.io/rem/` | `site/index.html` (`og:url`), `README.md` |
| Support URL | `https://rem-sync.shettyh.fyi/support` | `ios/RemApp/Theme/RemTokens.swift` (`RemDocs.supportURL`) |
| Privacy Policy URL | `https://rem-sync.shettyh.fyi/privacy` | `ios/RemApp/Theme/RemTokens.swift` (`RemDocs.privacyPolicyURL`) |
| Terms of Use (EULA) | `https://rem-sync.shettyh.fyi/terms` | `ios/RemApp/Theme/RemTokens.swift` (`RemDocs.termsOfUseURL`) |

---

## App Privacy Summary (Synced with `PrivacyInfo.xcprivacy`)

- **Tracking:** No (`NSPrivacyTracking = false`, no tracking domains).
- **Data Linked to You (App Functionality only, when optional Cloud Sync is enabled):**
  - Contact Info → Email Address (`NSPrivacyCollectedDataTypeEmailAddress`)
  - Identifiers → User ID (`NSPrivacyCollectedDataTypeUserID`)
  - User Content → Other User Content (`NSPrivacyCollectedDataTypeOtherUserContent` — synced decks, cards, review logs)
  - User Content → Photos or Videos (`NSPrivacyCollectedDataTypePhotosorVideos` — synced card images & diagrams)
  - Purchases → Purchase History (`NSPrivacyCollectedDataTypePurchaseHistory` — App Store subscription verification)
- **Export Compliance:** `ITSAppUsesNonExemptEncryption = false` (uses only standard OS TLS/HTTPS and Keychain).

---

## In-App Purchase Subscription Metadata

Mirrors `ios/RemAppTests/RemPremium.storekit`, `ios/RemApp/Features/Settings/StoreKitManager.swift`, and `ios/RemApp/Features/Settings/PremiumPaywallSheet.swift`:

| Field | Value |
| --- | --- |
| Product ID | `com.shettyh.rem.premium.yearly` |
| Reference Name | `rem Premium Annual` |
| Subscription Group | `rem Premium` (StoreKit local ID `21500001`) |
| Duration | 1 Year (`P1Y`, Auto-Renewable) |
| Price (USD Tier) | `$4.99 / year` |
| Family Sharing | Off (`false`) |
| Introductory Offer (App Store) | None (`null` — a 30-day no-card account trial is available directly inside Settings after sign-in) |
| Localization (`en_US`) — Group Display Name | `rem Premium` |
| Localization (`en_US`) — Group Description | `Real-time Cloud Sync for decks, progress, and media (images, diagrams & audio) across desktop, CLI, and iPhone.` |
| Localization (`en_US`) — Subscription Display Name | `rem Premium Annual` |
| Localization (`en_US`) — Subscription Description | `Real-time Cloud Sync with 500 MB media storage (images, diagrams & audio) across Mac, Linux, Windows, CLI, and iPhone.` |

### App Review Notes (Paste into App Store Connect → App Review Information)

```text
rem works 100% offline with no sign-in required. Tap "Start locally" on the welcome screen to create a deck, import an Anki .apkg file, create cards (including Cloze and Image Occlusion), study with FSRS-7, and inspect Stats immediately.

Optional Cloud Sync ("rem Premium"):
1. Open the Settings tab and sign in with Apple or Google (no separate account registration needed).
2. New accounts can start a 30-day free trial (no card required) or subscribe directly via the native StoreKit 2 paywall ($4.99/year, com.shettyh.rem.premium.yearly), which displays the localized App Store price, auto-renewal disclosure, Restore Purchases, Privacy Policy, and Terms of Use links.
3. Users can also delete their cloud account and synced server data directly from Settings → Account → Delete cloud account while keeping local cards intact on the device.
```

---

## 6-Screen App Store Screenshot Storyboard (iPhone 6.9" & 6.5")

All screenshots should use the **Quiet Paper Light** theme (plus one optional dark-mode accent or consistent warm-paper backdrop `#F5F4F2` with indigo `#5A47E8` accents), crisp status bar (`9:41`, full signal/battery), and realistic, high-signal study content (e.g., Anatomy / Medicine, Systems Engineering, and Japanese).

| # | Headline (Top Caption) | Subhead | iOS Screen & Content to Capture |
| --- | --- | --- | --- |
| **1** | **Calm, adaptive spaced repetition.** | FSRS-7 scheduling that works 100% offline—free forever. | **`TodayView`** with an active due card (`24 cards due · 6 new · 18 review · ~5 min`, `14d streak` badge, `Start review →` button) and clean nested decks below (`Medicine` folder expanded into `Anatomy`, `Pharmacology`; `Systems`; `Japanese`) showing color dots and `due` pills. |
| **2** | **Study with Image Occlusion.** | Hide labels on diagrams, tap to reveal, and zoom in cleanly. | **`ReviewScreen`** displaying an `IMAGE OCCLUSION · #1` card (e.g., cranial nerves or heart anatomy diagram) in revealed or pre-reveal state with crisp indigo active mask, muted inactive masks, and the 4-button FSRS grade dock (`Again`, `Hard`, `Good`, `Easy`) with real intervals (`10m`, `1d`, `4d`, `9d`). |
| **3** | **Cloze, LaTeX math & audio.** | Native formatting for STEM, medicine, code, and languages. | **`ReviewScreen`** (revealed) showing a card combining Cloze pill reveal (`{{c1::...}}`), a crisp native LaTeX block formula (`MathBlockView`), an inline audio clip (`AssetAudioPlayerView`) or dark syntax-highlighted code block, `#tags`, and the collapsed `Source & rationale` provenance disclosure. |
| **4** | **Bring your Anki decks.** | Import .apkg files on iPhone with media, subdecks & history. | **`DecksView`** showing nested collapsible subdecks (`Medicine / Anatomy`, `Languages / Japanese`) alongside the search bar (`Search decks and cards`) or the native `Import Anki deck?` preview alert (`Found 3 decks, 420 cards, 85 media, 1,240 review logs`) proving zero-friction migration. |
| **5** | **Capture & occlude in seconds.** | Markdown editor, Cloze shortcuts, and built-in mask drawing. | **`QuickCaptureSheet`** or **`ImageOcclusionEditorSheet`** showing the clean paper canvas with Rect/Ellipse mask tools and `Hide all` / `Hide one` controls over a diagram (or Markdown editor with `[…]` Cloze insertion and morphing formatting bar). |
| **6** | **Track retention. Sync anywhere.** | Interactive Swift Charts stats + optional real-time Cloud Sync. | **`StatsView`** (30D range) showing the 4 KPI cards (`412 Reviews`, `92% Recall rate`, `14 days Current streak`, `26 Active days`), interactive Swift Charts `Review activity` bar chart with a scrubbed day tooltip, `Grade distribution` bars, and `By deck` breakdown. |
