#!/usr/bin/env node
/**
 * Unit tests for release tooling (`generate-updater-manifest.mjs` and `render-homebrew-cask.mjs`).
 */
import assert from 'node:assert/strict'
import { buildUpdaterManifest } from './generate-updater-manifest.mjs'
import { renderHomebrewCask } from './render-homebrew-cask.mjs'

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
