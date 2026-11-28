# Changelog

## 0.31.0 (2026-11-28)

## What's Changed
* refactor: remove TypeScript shadow engine and freeze CLI study scope by @shettyh in https://github.com/shettyh/rem/pull/281
* fix(desktop): relaunch macOS app in foreground after auto-update by @shettyh in https://github.com/shettyh/rem/pull/284
* feat(ui,ios): add Anki (.apkg) and JSON import to deck creation flows by @shettyh in https://github.com/shettyh/rem/pull/283


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.30.2...v0.31.0

## 0.30.2 (2026-11-28)

## What's Changed
* fix(sync-server): track asset quota in O(1) SQLite row reads and handle DO/R2 storage errors by @shettyh in https://github.com/shettyh/rem/pull/279


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.30.1...v0.30.2

## 0.30.1 (2026-11-28)

## What's Changed
* fix(anki,ui,ios): polish nested deck tree, cascading subtree delete, and Anki card rendering by @shettyh in https://github.com/shettyh/rem/pull/277


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.30.0...v0.30.1

## 0.30.0 (2026-11-28)

## What's Changed
* feat: Markdown-native Image Occlusion across core, Desktop, iOS, TUI, and .apkg import by @shettyh in https://github.com/shettyh/rem/pull/274
* fix: harden Rust core, sync client, and add property/simulation tests by @shettyh in https://github.com/shettyh/rem/pull/276


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.29.1...v0.30.0

## 0.29.1 (2026-11-28)

## What's Changed
* fix(ui): eliminate SearchModal debounce race in browser tests by @shettyh in https://github.com/shettyh/rem/pull/272


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.29.0...v0.29.1

## 0.29.0 (2026-11-28)

## What's Changed
* feat: Wave 1 Anki compatibility & migration (.apkg, cloze, LaTeX, nested decks, audio, timer) by @shettyh in https://github.com/shettyh/rem/pull/269
* fix: harden Wave 1 Anki compatibility across Rust core, Desktop/Web, CLI, and iOS by @shettyh in https://github.com/shettyh/rem/pull/271


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.28.0...v0.29.0

## 0.28.0 (2026-11-27)

## What's Changed
* feat(core,desktop,cli,ios): Phase 2 daily-driver parity, draft provenance/triage, FTS5 search, and unified stats by @shettyh in https://github.com/shettyh/rem/pull/267


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.27.1...v0.28.0

## 0.27.1 (2026-11-26)

## What's Changed
* build(deps-dev): bump @tauri-apps/cli from 2.12.0 to 2.12.6 in the npm-minor-and-patch group by @dependabot[bot] in https://github.com/shettyh/rem/pull/218


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.27.0...v0.27.1

## 0.27.0 (2026-11-26)

## What's Changed
* feat(desktop): add automatic background updates setting and sidebar restart prompt by @shettyh in https://github.com/shettyh/rem/pull/264


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.26.1...v0.27.0

## 0.26.1 (2026-11-25)

## What's Changed
* chore: remove unused invite code and redemption support by @shettyh in https://github.com/shettyh/rem/pull/262


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.26.0...v0.26.1

## 0.26.0 (2026-11-25)

## What's Changed
* feat(skill): improve card-writing guidance for cleaner, higher-recall cards by @shettyh in https://github.com/shettyh/rem/pull/258
* feat(ios): use native SwiftUI buttons and onboarding layout by @shettyh in https://github.com/shettyh/rem/pull/260
* docs(site,readme): highlight Agent Skill + CLI workflow and refresh product details by @shettyh in https://github.com/shettyh/rem/pull/261


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.25.6...v0.26.0

## 0.25.6 (2026-11-25)

## What's Changed
* fix(core,ui,ios): align default deck learning config and step Hard behavior with Anki/FSRS by @shettyh in https://github.com/shettyh/rem/pull/255
* fix(sync,ui,ios): serialize token refresh and fix Settings/Today layout alignment by @shettyh in https://github.com/shettyh/rem/pull/257


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.25.5...v0.25.6

## 0.25.5 (2026-11-24)

## What's Changed
* fix(ci): keep temp wrangler config in sync-server/ and preserve dashboard vars by @shettyh in https://github.com/shettyh/rem/pull/253


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.25.4...v0.25.5

## 0.25.4 (2026-11-24)

## What's Changed
* fix(core,sync): atomic undo rollback, sync-server hardening, and site refresh by @shettyh in https://github.com/shettyh/rem/pull/251


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.25.3...v0.25.4

## 0.25.3 (2026-11-24)

## What's Changed
* fix(ios): align privacy manifest data types and add Settings support link by @shettyh in https://github.com/shettyh/rem/pull/249


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.25.2...v0.25.3

## 0.25.2 (2026-11-24)

## What's Changed
* fix(ci): skip headless SKTestSession purchase test and add CI timeouts by @shettyh in https://github.com/shettyh/rem/pull/247


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.25.1...v0.25.2

## 0.25.1 (2026-11-24)

## What's Changed
* fix(ios,sync): add DiskSpace privacy manifest entry and App Store /support page by @shettyh in https://github.com/shettyh/rem/pull/245


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.25.0...v0.25.1

## 0.25.0 (2026-11-24)

## What's Changed
* feat(ios): redesign Settings Cloud Sync section with Account drill-down by @shettyh in https://github.com/shettyh/rem/pull/242
* fix(ios): move Today sync status inline with date header by @shettyh in https://github.com/shettyh/rem/pull/244


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.24.0...v0.25.0

## 0.24.0 (2026-11-23)

## What's Changed
* feat(ios): adopt native forms, lists, toolbars, Swift Charts, and UIScrollView lightbox by @shettyh in https://github.com/shettyh/rem/pull/238
* fix(ios): fix Review activity chart X-axis overlap and empty-state scale by @shettyh in https://github.com/shettyh/rem/pull/239
* feat(ios): redesign Study screen and unify native tab surfaces by @shettyh in https://github.com/shettyh/rem/pull/240


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.23.0...v0.24.0

## 0.23.0 (2026-11-23)

## What's Changed
* feat(ios): use native TabView and top-right toolbar Quick Capture by @shettyh in https://github.com/shettyh/rem/pull/236


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.22.1...v0.23.0

## 0.22.1 (2026-11-23)

## What's Changed
* fix(ios): preserve Sign in with Apple entitlement in TestFlight archives by @shettyh in https://github.com/shettyh/rem/pull/234


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.22.0...v0.22.1

## 0.22.0 (2026-11-22)

## What's Changed
* fix(ios,sync): App Store review readiness (3.1.1, 5.1.1(v), ITMS-91053) by @shettyh in https://github.com/shettyh/rem/pull/231
* feat(ios,sync): native Sign in with Apple on iOS (Guideline 4.8) by @shettyh in https://github.com/shettyh/rem/pull/232


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.21.0...v0.22.0

## 0.21.0 (2026-11-22)

## What's Changed
* feat(ios): add deck editing and full FSRS deck options by @shettyh in https://github.com/shettyh/rem/pull/229


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.20.0...v0.21.0

## 0.20.0 (2026-11-22)

## What's Changed
* ci(ios): add iOS CI tests and automated TestFlight distribution by @shettyh in https://github.com/shettyh/rem/pull/223
* fix(ios): set TARGETED_DEVICE_FAMILY=1 on iOS targets for App Store Connect by @shettyh in https://github.com/shettyh/rem/pull/225
* feat(sync): rem Premium unified entitlements, 30-day free trial, invite codes & Cloud Sync UI (Slice 1) by @shettyh in https://github.com/shettyh/rem/pull/224
* feat(sync): Polar checkout, customer portal, and webhook billing (Slice 2) by @shettyh in https://github.com/shettyh/rem/pull/227
* feat(ios,sync): iOS StoreKit 2 IAP & Apple Server Notifications v2 (Slice 3) by @shettyh in https://github.com/shettyh/rem/pull/228


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.19.0...v0.20.0

## 0.19.0 (2026-11-22)

## What's Changed
* feat(release): bundle rem CLI, add Tauri auto-updater, and automate Homebrew tap by @shettyh in https://github.com/shettyh/rem/pull/221


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.18.0...v0.19.0

## 0.18.0 (2026-11-22)

## What's Changed
* docs(site): redesign site with Desktop + iPhone slide showcase and streamline README by @shettyh in https://github.com/shettyh/rem/pull/188
* fix(ci): split GitHub Pages build and deploy jobs by @shettyh in https://github.com/shettyh/rem/pull/190
* build(deps): bump the codeql group with 2 updates by @dependabot[bot] in https://github.com/shettyh/rem/pull/191
* build(deps): bump tauri from 2.12.4 to 2.12.5 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/193
* build(deps): bump base64 from 0.22.1 to 0.23.1 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/194
* build(deps): bump tauri-plugin-log from 2.10.1 to 2.10.2 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/195
* build(deps): bump clap from 4.6.7 to 4.6.8 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/198
* build(deps): bump rand from 0.10.3 to 0.10.4 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/199
* build(deps): bump log from 0.4.34 to 0.4.35 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/200
* build(deps): bump the tiptap group with 7 updates by @dependabot[bot] in https://github.com/shettyh/rem/pull/201
* build(deps-dev): bump jsdom from 30.1.1 to 31.0.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/202
* build(deps-dev): bump vitest from 5.0.5 to 5.1.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/206
* build(deps-dev): bump playwright from 1.63.0 to 1.65.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/204
* build(deps-dev): bump fake-indexeddb from 6.2.5 to 6.2.8 by @dependabot[bot] in https://github.com/shettyh/rem/pull/203
* build(deps): bump react and react-dom to 19.3.1 and group Dependabot updates by @shettyh in https://github.com/shettyh/rem/pull/207
* build(deps): bump uniffi from 0.29.5 to 0.32.2 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/197
* build(deps-dev): bump @testing-library/jest-dom from 7.0.1 to 8.0.0 by @dependabot[bot] in https://github.com/shettyh/rem/pull/208
* build(deps): bump dexie from 4.4.6 to 4.4.7 by @dependabot[bot] in https://github.com/shettyh/rem/pull/209
* build(deps): bump tauri from 2.12.5 to 2.12.7 in /src-tauri in the tauri group by @dependabot[bot] in https://github.com/shettyh/rem/pull/210
* build(deps-dev): bump @vitejs/plugin-react from 6.1.1 to 6.3.0 by @dependabot[bot] in https://github.com/shettyh/rem/pull/211
* build(deps-dev): bump @testing-library/user-event from 14.6.7 to 14.6.13 by @dependabot[bot] in https://github.com/shettyh/rem/pull/212
* build(deps): bump uuid from 1.29.0 to 1.29.3 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/213
* build(deps): bump react-router-dom from 7.18.4 to 7.18.6 by @dependabot[bot] in https://github.com/shettyh/rem/pull/214
* build(deps-dev): bump vite from 8.3.4 to 8.3.8 by @dependabot[bot] in https://github.com/shettyh/rem/pull/215
* build(deps): bump reqwest from 0.13.4 to 0.13.5 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/216
* ci(dependabot): group routine dependency updates into weekly PRs by @shettyh in https://github.com/shettyh/rem/pull/217
* feat(scheduler): migrate to fsrs 7.0.0 (FSRS-7) by @shettyh in https://github.com/shettyh/rem/pull/219
* ci(macos): add Apple Developer ID signing and notarization pipeline by @shettyh in https://github.com/shettyh/rem/pull/220


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.17.0...v0.18.0

## 0.17.0 (2026-11-21)

## What's Changed
* feat(ios): add native SwiftUI iOS companion app with shared Rust core (Slice 4) by @shettyh in https://github.com/shettyh/rem/pull/184
* feat(ux): polish iOS and desktop UI copy, decks, card editor, and auth gate by @shettyh in https://github.com/shettyh/rem/pull/186
* docs: refresh vision and roadmap for Cloud Sync and iOS companion v1 by @shettyh in https://github.com/shettyh/rem/pull/187


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.16.0...v0.17.0

## 0.16.0 (2026-11-21)

## What's Changed
* chore: remove legacy git sync by @shettyh in https://github.com/shettyh/rem/pull/177
* feat(core): add cloud sync engine (Slice 1) by @shettyh in https://github.com/shettyh/rem/pull/179
* feat(sync): add Cloudflare sync-server and rem-sync crate (Slice 2) by @shettyh in https://github.com/shettyh/rem/pull/180
* feat(sync): add OAuth/Keychain auth, desktop UI, and CLI sync (Slice 3) by @shettyh in https://github.com/shettyh/rem/pull/181
* feat(sync): default to hosted rem Cloud UX and add server entitlement gate by @shettyh in https://github.com/shettyh/rem/pull/182
* feat(sync): open upgrade URL natively, add /cloud waitlist, and refresh sync docs by @shettyh in https://github.com/shettyh/rem/pull/183


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.15.0...v0.16.0

## 0.15.0 (2026-11-21)

## What's Changed
* docs: refresh README and website screenshots after UI redesign by @shettyh in https://github.com/shettyh/rem/pull/174
* feat(ui): collapsible icon-rail sidebar, dense cards list, editor polish, and deck progress ring by @shettyh in https://github.com/shettyh/rem/pull/176


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.14.0...v0.15.0

## 0.14.0 (2026-11-20)

## What's Changed
* build(deps): bump clap from 4.6.6 to 4.6.7 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/136
* build(deps): bump react-router-dom from 7.18.3 to 7.18.4 by @dependabot[bot] in https://github.com/shettyh/rem/pull/137
* build(deps-dev): bump playwright from 1.62.1 to 1.63.0 by @dependabot[bot] in https://github.com/shettyh/rem/pull/139
* build(deps): bump react-dom and @types/react-dom by @dependabot[bot] in https://github.com/shettyh/rem/pull/140
* build(deps): bump the codeql group with 2 updates by @dependabot[bot] in https://github.com/shettyh/rem/pull/141
* build(deps): bump rand from 0.10.1 to 0.10.3 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/142
* fix(deps): apply the pending npm and Rust security updates by @shettyh in https://github.com/shettyh/rem/pull/171
* build(deps): bump uuid from 1.26.1 to 1.29.0 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/167
* build(deps): bump sha2 from 0.11.0 to 0.11.1 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/158
* build(deps): bump serde_json from 1.0.151 to 1.0.154 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/160
* build(deps): upgrade Tauri, TipTap, and rusqlite together by @shettyh in https://github.com/shettyh/rem/pull/172
* feat(ui): elevate design cohesion, study motivation, and everyday ergonomics by @shettyh in https://github.com/shettyh/rem/pull/173


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.13.1...v0.14.0

## 0.13.1 (2026-09-19)

## What's Changed
* build(deps): bump actions/configure-pages from 5 to 6 by @dependabot[bot] in https://github.com/shettyh/rem/pull/112
* build(deps): bump actions/deploy-pages from 4 to 5 by @dependabot[bot] in https://github.com/shettyh/rem/pull/111
* build(deps): bump the tiptap group with 7 updates by @dependabot[bot] in https://github.com/shettyh/rem/pull/115
* build(deps): bump rand from 0.8.7 to 0.10.1 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/99
* build(deps): bump rusqlite from 0.32.1 to 0.40.2 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/101
* build(deps): bump pulldown-cmark from 0.10.3 to 0.13.4 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/103
* build(deps): bump sha2 from 0.10.9 to 0.11.0 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/104
* build(deps-dev): bump @vitest/browser-playwright from 4.1.10 to 4.1.11 by @dependabot[bot] in https://github.com/shettyh/rem/pull/116
* build(deps): bump dexie from 4.4.4 to 4.4.6 by @dependabot[bot] in https://github.com/shettyh/rem/pull/117
* build(deps-dev): bump Vitest packages to 5.0.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/118
* build(deps): bump the codeql group with 2 updates by @dependabot[bot] in https://github.com/shettyh/rem/pull/119
* build(deps): bump log from 0.4.33 to 0.4.34 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/122
* build(deps-dev): bump vite from 8.2.1 to 8.3.0 by @dependabot[bot] in https://github.com/shettyh/rem/pull/124
* build(deps): bump uuid from 1.24.0 to 1.26.1 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/127
* build(deps): bump react-router-dom from 7.18.2 to 7.18.3 by @dependabot[bot] in https://github.com/shettyh/rem/pull/128
* build(deps): bump fsrs from 6.6.1 to 6.6.2 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/129
* build(deps-dev): bump @testing-library/react from 16.3.2 to 16.3.3 by @dependabot[bot] in https://github.com/shettyh/rem/pull/130
* build(deps): bump tauri-plugin-log from 2.9.0 to 2.9.1 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/131
* build(deps-dev): bump @vitejs/plugin-react from 6.0.5 to 6.1.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/132
* build(deps-dev): bump @testing-library/user-event from 14.6.4 to 14.6.7 by @dependabot[bot] in https://github.com/shettyh/rem/pull/133


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.13.0...v0.13.1

## 0.13.0 (2026-08-16)

## What's Changed
* feat(review): improve rich card readability by @shettyh in https://github.com/shettyh/rem/pull/96
* build(deps): bump actions/upload-pages-artifact from 4 to 5 by @dependabot[bot] in https://github.com/shettyh/rem/pull/113
* build(deps-dev): bump vite from 8.2.0 to 8.2.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/110
* build(deps-dev): bump @testing-library/jest-dom from 7.0.0 to 7.0.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/109
* build(deps-dev): bump @testing-library/user-event from 14.6.3 to 14.6.4 by @dependabot[bot] in https://github.com/shettyh/rem/pull/108
* build(deps): bump highlight.js from 11.11.1 to 11.12.0 by @dependabot[bot] in https://github.com/shettyh/rem/pull/107
* build(deps): bump the tiptap group with 7 updates by @dependabot[bot] in https://github.com/shettyh/rem/pull/106
* build(deps): bump uuid from 1.23.3 to 1.24.0 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/105
* build(deps): bump directories from 5.0.1 to 6.0.0 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/102
* build(deps): bump clap from 4.5.4 to 4.6.6 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/100
* build(deps): bump crossterm from 0.27.0 to 0.29.0 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/98


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.12.2...v0.13.0

## 0.12.2 (2026-08-10)

## What's Changed
* fix(deps): resolve lru security advisory by @shettyh in https://github.com/shettyh/rem/pull/94


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.12.1...v0.12.2

## 0.12.1 (2026-08-10)

## What's Changed
* fix(skill): strengthen rem capture policy by @shettyh in https://github.com/shettyh/rem/pull/92


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.12.0...v0.12.1

## 0.12.0 (2026-08-10)

## What's Changed
* fix(site): preserve hero image aspect ratio by @shettyh in https://github.com/shettyh/rem/pull/86
* fix(site): make screenshots responsive by @shettyh in https://github.com/shettyh/rem/pull/87
* docs(site): sharpen AI-assisted work pitch by @shettyh in https://github.com/shettyh/rem/pull/88
* feat(review): improve study reading layout by @shettyh in https://github.com/shettyh/rem/pull/89
* docs(site): broaden durable understanding pitch by @shettyh in https://github.com/shettyh/rem/pull/91


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.11.1...v0.12.0

## 0.11.1 (2026-08-09)

## What's Changed
* docs: add GitHub Pages website by @shettyh in https://github.com/shettyh/rem/pull/84


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.11.0...v0.11.1

## 0.11.0 (2026-08-09)

## What's Changed
* feat: distribute the agent skill universally by @shettyh in https://github.com/shettyh/rem/pull/82


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.10.1...v0.11.0

## 0.10.1 (2026-08-09)

## What's Changed
* docs: showcase terminal study workflow by @shettyh in https://github.com/shettyh/rem/pull/79
* docs: showcase desktop draft approval by @shettyh in https://github.com/shettyh/rem/pull/81


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.10.0...v0.10.1

## 0.10.0 (2026-08-09)

## What's Changed
* feat(ui): add semantic sidebar icons by @shettyh in https://github.com/shettyh/rem/pull/77


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.9.0...v0.10.0

## 0.9.0 (2026-08-09)

## What's Changed
* feat: add agent-first learning loop by @shettyh in https://github.com/shettyh/rem/pull/75


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.8.0...v0.9.0

## 0.8.0 (2026-08-09)

## What's Changed
* feat: add native SQLite storage and CLI capture by @shettyh in https://github.com/shettyh/rem/pull/73


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.7.0...v0.8.0

## 0.7.0 (2026-08-08)

## What's Changed
* feat(ui): show app version in settings by @shettyh in https://github.com/shettyh/rem/pull/71


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.6.4...v0.7.0

## 0.6.4 (2026-08-08)

## What's Changed
* build(deps-dev): bump @testing-library/user-event from 14.6.1 to 14.6.3 by @dependabot[bot] in https://github.com/shettyh/rem/pull/65
* build(deps-dev): bump jsdom from 29.1.1 to 30.0.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/67


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.6.3...v0.6.4

## 0.6.3 (2026-08-08)

## What's Changed
* build(deps): bump actions/setup-node from 4 to 7 by @dependabot[bot] in https://github.com/shettyh/rem/pull/40
* build(deps): bump actions/checkout from 4 to 7 by @dependabot[bot] in https://github.com/shettyh/rem/pull/53
* build(deps): bump tauri-apps/tauri-action from 0 to 1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/55
* build(deps): coordinate coupled dependency updates by @shettyh in https://github.com/shettyh/rem/pull/64
* build(deps-dev): bump typescript from 6.0.3 to 7.0.2 by @dependabot[bot] in https://github.com/shettyh/rem/pull/69
* build(deps-dev): bump @types/react from 19.2.17 to 19.2.18 by @dependabot[bot] in https://github.com/shettyh/rem/pull/68
* build(deps-dev): bump @testing-library/jest-dom from 6.9.1 to 7.0.0 by @dependabot[bot] in https://github.com/shettyh/rem/pull/66


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.6.2...v0.6.3

## 0.6.2 (2026-08-08)

## What's Changed
* build(deps): bump react-dom and @types/react-dom by @dependabot[bot] in https://github.com/shettyh/rem/pull/59
* build(deps): bump googleapis/release-please-action from 4 to 5 by @dependabot[bot] in https://github.com/shettyh/rem/pull/56
* build(deps-dev): bump playwright from 1.61.0 to 1.62.1 by @dependabot[bot] in https://github.com/shettyh/rem/pull/51
* build(deps): bump actions/upload-artifact from 4 to 7 by @dependabot[bot] in https://github.com/shettyh/rem/pull/41
* build(deps): bump tauri from 2.11.3 to 2.11.5 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/42
* build(deps): bump serde_json from 1.0.150 to 1.0.151 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/46
* build(deps-dev): bump @tauri-apps/cli from 2.11.3 to 2.11.4 by @dependabot[bot] in https://github.com/shettyh/rem/pull/50
* build(deps): bump serde from 1.0.228 to 1.0.229 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/44
* build(deps-dev): bump @vitejs/plugin-react from 6.0.2 to 6.0.5 by @dependabot[bot] in https://github.com/shettyh/rem/pull/47
* build(deps-dev): bump vite from 8.0.16 to 8.2.0 by @dependabot[bot] in https://github.com/shettyh/rem/pull/45
* build(deps): bump base64 from 0.22.1 to 0.23.1 in /src-tauri by @dependabot[bot] in https://github.com/shettyh/rem/pull/48

## New Contributors
* @dependabot[bot] made their first contribution in https://github.com/shettyh/rem/pull/59

**Full Changelog**: https://github.com/shettyh/rem/compare/v0.6.1...v0.6.2

## 0.6.1 (2026-08-08)

## What's Changed
* docs: align README project description by @shettyh in https://github.com/shettyh/rem/pull/36
* fix(deps): remediate known vulnerabilities by @shettyh in https://github.com/shettyh/rem/pull/38
* ci(security): automate vulnerability scanning by @shettyh in https://github.com/shettyh/rem/pull/39
* fix(security): harden Tauri sync boundaries by @shettyh in https://github.com/shettyh/rem/pull/61


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.6.0...v0.6.1

## 0.6.0 (2026-08-08)

## What's Changed
* feat(ui): refine review card and grading controls by @shettyh in https://github.com/shettyh/rem/pull/30
* feat(ui): simplify deck identity by @shettyh in https://github.com/shettyh/rem/pull/32
* fix(ui): align review grades and editor typography by @shettyh in https://github.com/shettyh/rem/pull/33
* docs: refresh README for quiet design by @shettyh in https://github.com/shettyh/rem/pull/34
* docs: refresh license badge cache by @shettyh in https://github.com/shettyh/rem/pull/35


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.5.0...v0.6.0

## 0.5.0 (2026-08-08)

## What's Changed
* fix(ui): normalize button fonts and header controls by @shettyh in https://github.com/shettyh/rem/pull/23
* fix(ui): clarify button variants and interaction states by @shettyh in https://github.com/shettyh/rem/pull/24
* fix(ui): align typography and empty states by @shettyh in https://github.com/shettyh/rem/pull/26
* feat(ui): establish a quieter visual foundation by @shettyh in https://github.com/shettyh/rem/pull/27
* feat(ui): simplify Today and deck surfaces by @shettyh in https://github.com/shettyh/rem/pull/28
* feat(ui): quiet review and card editing by @shettyh in https://github.com/shettyh/rem/pull/29


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.4.0...v0.5.0

## 0.4.0 (2026-08-08)

## What's Changed
* feat(settings): emphasize local storage and optional sync by @shettyh in https://github.com/shettyh/rem/pull/21


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.3.1...v0.4.0

## 0.3.1 (2026-08-08)

## What's Changed
* fix(ui): align native toolbar and narrow layouts by @shettyh in https://github.com/shettyh/rem/pull/19


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.3.0...v0.3.1

## 0.3.0 (2026-08-08)

## What's Changed
* feat(review): add focused study mode by @shettyh in https://github.com/shettyh/rem/pull/17


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.2.1...v0.3.0

## 0.2.1 (2026-08-08)

## What's Changed
* fix(ui): improve editor safety and consistency by @shettyh in https://github.com/shettyh/rem/pull/14
* fix(ui): improve readability and keyboard access by @shettyh in https://github.com/shettyh/rem/pull/16


**Full Changelog**: https://github.com/shettyh/rem/compare/v0.2.0...v0.2.1

## 0.2.0 (2026-08-08)

## What's Changed
* Anki-grade review-queue steps + per-deck FSRS params (#3a) by @shettyh in https://github.com/shettyh/rem/pull/1
* fix(review): close out #3a follow-ups — double-grade guard, reviews-count copy, scheduler cleanups by @shettyh in https://github.com/shettyh/rem/pull/2
* feat(review): daily caps (newPerDay/maxReviews) — sub-project #3b by @shettyh in https://github.com/shettyh/rem/pull/3
* Fix/review followup cleanups by @shettyh in https://github.com/shettyh/rem/pull/4
* test(review): verify daily cap persists across sessions by @shettyh in https://github.com/shettyh/rem/pull/5
* feat(review): enforce leech tag and suspend actions by @shettyh in https://github.com/shettyh/rem/pull/6
* feat(review): add custom study sessions by @shettyh in https://github.com/shettyh/rem/pull/7
* feat(fsrs): optimize per-deck weights by @shettyh in https://github.com/shettyh/rem/pull/8
* feat(stats): add review activity dashboard by @shettyh in https://github.com/shettyh/rem/pull/9
* Add user-defined card tags by @shettyh in https://github.com/shettyh/rem/pull/10
* docs: prepare rem for open source by @shettyh in https://github.com/shettyh/rem/pull/11
* ci: automate releases through pull requests by @shettyh in https://github.com/shettyh/rem/pull/12

## New Contributors
* @shettyh made their first contribution in https://github.com/shettyh/rem/pull/1

**Full Changelog**: https://github.com/shettyh/rem/compare/v0.1.0...v0.2.0
