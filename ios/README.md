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
   Useful flags:
   - `--debug` — debug Rust build (omit for optimized Release XCFramework)
   - `--sim-only` — build only `aarch64-apple-ios-sim` (faster for Simulator runs & tests)
   - `--device-only` — build only `aarch64-apple-ios` (faster for device/TestFlight archives)

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

## Versioning & App Store Readiness

- Marketing version lives in `ios/Config/Version.xcconfig` (`MARKETING_VERSION`) and is kept in sync with desktop/CLI releases by Release Please.
- `ios/RemApp/Resources/Info.plist` references `$(MARKETING_VERSION)` / `$(CURRENT_PROJECT_VERSION)` and sets `ITSAppUsesNonExemptEncryption = false` (only standard OS TLS/HTTPS and Keychain are used).
- `ios/RemApp/Resources/PrivacyInfo.xcprivacy` declares required-reason API usage (`NSPrivacyAccessedAPICategoryUserDefaults` reason `CA92.1`, `NSPrivacyAccessedAPICategoryFileTimestamp` reason `C617.1`, and `NSPrivacyAccessedAPICategoryDiskSpace` reason `E174.1`).
- `AppIcon-1024.png` is stored as opaque 24-bit RGB without an alpha channel (`ITMS-90717`).

## CI & TestFlight Distribution

### Continuous Integration (`.github/workflows/ci.yml`)

The `ios` CI job runs on `xcode-27` (selecting the latest pre-installed stable Xcode via `tools/select-latest-xcode.sh`, currently Xcode 27.2 / iOS 27.2 SDK) for every PR and push to `main`:
1. Builds `RemCore.xcframework` (`--debug --sim-only`) and regenerates UniFFI Swift bindings.
2. Runs `xcodebuild test -project ios/Rem.xcodeproj -scheme RemApp` (`RemAppTests`) on an available iPhone Simulator.

### TestFlight Uploads (`.github/workflows/ios-testflight.yml`)

TestFlight builds also run on `xcode-27` with the latest stable Xcode/iOS SDK and use **Xcode Cloud Signing** via App Store Connect API keys (`-allowProvisioningUpdates` + `-authenticationKeyPath`), so no `.p12` distribution certificate or manual provisioning profile secret is required.

Uploads run in two ways:
- **Automatically on release:** `.github/workflows/release.yml` calls `ios-testflight.yml` whenever Release Please creates a new `vX.Y.Z` release.
- **Manually on demand:** Trigger **Actions → iOS TestFlight → Run workflow** (or `gh workflow run ios-testflight.yml`). Supports optional `version`, `internal_only` (restricts to internal testers), and `dry_run` (archives & verifies without uploading).

#### One-time Apple Developer / App Store Connect Setup

1. **Bundle ID & App record:** Register App ID `com.shettyh.rem` in Apple Developer Portal and create the matching iOS app record in [App Store Connect](https://appstoreconnect.apple.com/).
2. **GitHub Actions secrets:** Reuse the same three App Store Connect API key secrets already used for macOS notarization:
   - `APPLE_API_KEY` — App Store Connect API Key ID
   - `APPLE_API_ISSUER` — App Store Connect Issuer ID (UUID)
   - `APPLE_API_KEY_PRIVATE_KEY` — contents of `AuthKey_<KEY_ID>.p8`
   - Optional: `APPLE_TEAM_ID` secret or repository variable to override the default Apple Developer Team ID (`VB28U48NHS`).
3. **API Key permissions:** Ensure the App Store Connect API key has the **App Manager** or **Admin** role (or **Developer** with **Access to Cloud Managed Distribution Certificate** enabled) so Xcode can cloud-sign and upload to TestFlight.
