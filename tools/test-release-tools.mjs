#!/usr/bin/env node
/**
 * Unit tests for release tooling (`generate-updater-manifest.mjs` and `render-homebrew-cask.mjs`).
 */
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { buildUpdaterManifest } from './generate-updater-manifest.mjs'
import { renderHomebrewCask } from './render-homebrew-cask.mjs'
import { renderIosExportOptions } from './render-ios-export-options.mjs'

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

// 1. Updater manifest builder
{
  const manifest = buildUpdaterManifest({
    version: 'v0.19.0',
    tag: 'v0.19.0',
    repo: 'shettyh/rem',
    pubDate: '2026-11-22T06:00:00Z',
    notes: 'Added in-app updates and single-install CLI.',
    files: {
      'rem_0.19.0_aarch64.app.tar.gz': 'sig-arm64',
      'rem_0.19.0_x64.app.tar.gz': 'sig-x64',
      'rem_0.19.0_amd64.AppImage': 'sig-linux',
      'rem_0.19.0_x64-setup.exe': 'sig-win-nsis',
      'rem_0.19.0_x64_en-US.msi': 'sig-win-msi',
    },
  })

  assert.equal(manifest.version, '0.19.0')
  assert.equal(manifest.pub_date, '2026-11-22T06:00:00Z')
  assert.equal(manifest.notes, 'Added in-app updates and single-install CLI.')
  assert.deepEqual(Object.keys(manifest.platforms).sort(), [
    'darwin-aarch64',
    'darwin-x86_64',
    'linux-x86_64',
    'windows-x86_64',
  ])
  assert.equal(
    manifest.platforms['darwin-aarch64'].url,
    'https://github.com/shettyh/rem/releases/download/v0.19.0/rem_0.19.0_aarch64.app.tar.gz',
  )
  assert.equal(manifest.platforms['darwin-aarch64'].signature, 'sig-arm64')
  assert.equal(
    manifest.platforms['darwin-x86_64'].url,
    'https://github.com/shettyh/rem/releases/download/v0.19.0/rem_0.19.0_x64.app.tar.gz',
  )
  assert.equal(
    manifest.platforms['windows-x86_64'].url,
    'https://github.com/shettyh/rem/releases/download/v0.19.0/rem_0.19.0_x64-setup.exe',
  )
  console.log('ok   - buildUpdaterManifest maps all four desktop targets')
}

// 2. Homebrew Cask renderer
{
  const armSha = 'a'.repeat(64)
  const intelSha = 'b'.repeat(64)
  const cask = renderHomebrewCask({
    version: 'v0.19.0',
    armSha256: armSha,
    intelSha256: intelSha,
  })

  assert.match(cask, /cask "rem" do/)
  assert.match(cask, /version "0\.19\.0"/)
  assert.match(cask, new RegExp(`arm:\\s+"${armSha}"`))
  assert.match(cask, new RegExp(`intel:\\s+"${intelSha}"`))
  assert.match(cask, /auto_updates true/)
  assert.match(cask, /app "rem\.app"/)
  assert.match(cask, /binary "#\{appdir\}\/rem\.app\/Contents\/MacOS\/rem"/)
  console.log('ok   - renderHomebrewCask renders dual-arch notarized cask with bundled CLI binary')
}

// 3. Invalid SHA-256 rejection
{
  assert.throws(
    () =>
      renderHomebrewCask({
        version: '0.19.0',
        armSha256: 'not-a-sha',
        intelSha256: 'b'.repeat(64),
      }),
    /Invalid arm64 SHA-256/,
  )
  console.log('ok   - renderHomebrewCask rejects malformed checksums')
}

// 4. iOS ExportOptions.plist renderer
{
  const plist = renderIosExportOptions({
    teamId: 'VB28U48NHS',
    destination: 'upload',
    internalOnly: false,
  })
  assert.match(plist, /<key>destination<\/key>\s*<string>upload<\/string>/)
  assert.match(plist, /<key>method<\/key>\s*<string>app-store-connect<\/string>/)
  assert.match(plist, /<key>signingStyle<\/key>\s*<string>automatic<\/string>/)
  assert.match(plist, /<key>teamID<\/key>\s*<string>VB28U48NHS<\/string>/)
  assert.match(plist, /<key>manageAppVersionAndBuildNumber<\/key>\s*<true\/>/)
  assert.match(plist, /<key>uploadSymbols<\/key>\s*<true\/>/)
  assert.doesNotMatch(plist, /testFlightInternalTestingOnly/)

  const internalPlist = renderIosExportOptions({
    teamId: 'vb28u48nhs',
    destination: 'upload',
    internalOnly: true,
  })
  assert.match(internalPlist, /<key>teamID<\/key>\s*<string>VB28U48NHS<\/string>/)
  assert.match(internalPlist, /<key>testFlightInternalTestingOnly<\/key>\s*<true\/>/)

  assert.throws(
    () => renderIosExportOptions({ teamId: 'bad-id' }),
    /Invalid Apple Team ID/,
  )
  assert.throws(
    () => renderIosExportOptions({ teamId: 'VB28U48NHS', destination: 'ftp' }),
    /Invalid export destination/,
  )
  console.log('ok   - renderIosExportOptions generates valid TestFlight cloud-signing plists')
}

// 5. iOS App Store / TestFlight readiness invariants
{
  const versionXcconfig = fs.readFileSync(
    path.join(ROOT_DIR, 'ios/Config/Version.xcconfig'),
    'utf8',
  )
  assert.match(versionXcconfig, /MARKETING_VERSION\s*=\s*\d+\.\d+\.\d+/)
  assert.match(versionXcconfig, /CURRENT_PROJECT_VERSION\s*=\s*\d+/)

  const infoPlist = fs.readFileSync(
    path.join(ROOT_DIR, 'ios/RemApp/Resources/Info.plist'),
    'utf8',
  )
  assert.match(infoPlist, /<key>CFBundleShortVersionString<\/key>\s*<string>\$\(MARKETING_VERSION\)<\/string>/)
  assert.match(infoPlist, /<key>CFBundleVersion<\/key>\s*<string>\$\(CURRENT_PROJECT_VERSION\)<\/string>/)
  assert.match(infoPlist, /<key>ITSAppUsesNonExemptEncryption<\/key>\s*<false\/>/)

  const privacyManifest = fs.readFileSync(
    path.join(ROOT_DIR, 'ios/RemApp/Resources/PrivacyInfo.xcprivacy'),
    'utf8',
  )
  assert.match(privacyManifest, /NSPrivacyAccessedAPICategoryUserDefaults/)
  assert.match(privacyManifest, /<string>CA92\.1<\/string>/)

  // App Store Connect (ITMS-90717) rejects 1024x1024 AppIcon PNGs containing an alpha channel.
  // PNG IHDR sits at bytes 8..32; byte 25 is the PNG color type (2 = Truecolour RGB, no alpha).
  const iconPng = fs.readFileSync(
    path.join(ROOT_DIR, 'ios/RemApp/Resources/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png'),
  )
  assert.equal(iconPng.subarray(0, 8).toString('hex'), '89504e470d0a1a0a')
  assert.equal(iconPng.subarray(12, 16).toString('ascii'), 'IHDR')
  assert.equal(iconPng.readUInt32BE(16), 1024)
  assert.equal(iconPng.readUInt32BE(20), 1024)
  assert.equal(iconPng[25], 2, 'AppIcon-1024.png must be opaque RGB (PNG color type 2) without alpha')
  console.log('ok   - iOS Info.plist, PrivacyInfo.xcprivacy, Version.xcconfig, and opaque AppIcon are TestFlight-ready')
}
