# rem iOS Companion

Native SwiftUI iPhone companion app for `rem`, sharing the Rust `rem-core` (SQLite WAL + FSRS-7 `StudySession`) and `rem-sync` (Cloudflare Cloud Sync) crates via UniFFI (`src-tauri/crates/rem-ffi`).

## Prerequisites

- Xcode 16+ with iOS 17+ SDK
- Rust toolchain with iOS targets:
  ```sh
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim
  ```
- Optional (only if editing `ios/project.yml`): `brew install xcodegen`

## Build & Run Locally

1. Build `RemCore.xcframework` and generate Swift UniFFI bindings:
   ```sh
   bash tools/build-ios-xcframework.sh --debug
   ```
   *(Omit `--debug` for an optimized release XCFramework.)*

2. Open the Xcode project:
   ```sh
   open ios/Rem.xcodeproj
   ```
   Select an iPhone Simulator (or your physical iPhone once your Apple Developer team is configured under **Signing & Capabilities**) and press **Run (`⌘R`)**.

## Regenerating the Xcode Project

If you add or move Swift files or update `ios/project.yml`:

```sh
xcodegen generate --spec ios/project.yml
```
