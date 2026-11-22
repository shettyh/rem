#!/usr/bin/env node
/**
 * Generates Tauri v2's `latest.json` updater manifest from a directory of
 * downloaded release assets (`.tar.gz`, `.AppImage`, `.exe`, `.msi`, and `.sig` files).
 *
 * Usage:
 *   node tools/generate-updater-manifest.mjs \
 *     --version 0.19.0 \
 *     --tag v0.19.0 \
 *     --repo shettyh/rem \
 *     --assets-dir /tmp/release-assets \
 *     --notes-file /tmp/notes.md \
 *     --output /tmp/latest.json
 */
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

function parseArgs(argv) {
  const args = {}
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i]
    if (!key.startsWith('--')) throw new Error(`Unexpected argument: ${key}`)
    const val = argv[i + 1]
    if (val === undefined) throw new Error(`Missing value for ${key}`)
    args[key.slice(2)] = val
    i += 1
  }
  return args
}

/**
 * Match release asset filenames to Tauri v2 updater platform keys.
 * Every matched bundle must have a corresponding `<filename>.sig` file.
 */
export function buildUpdaterManifest({
  version,
  tag,
  repo,
  pubDate = new Date().toISOString(),
  notes = '',
  files, // Record<filename, signatureText>
}) {
  const cleanVersion = version.replace(/^v/, '')
  const baseUrl = `https://github.com/${repo}/releases/download/${tag}`
  const names = Object.keys(files).sort()

  const pick = (patterns) => {
    for (const pattern of patterns) {
      const match = names.find((name) => pattern.test(name))
      if (match) return match
    }
    return null
  }

  const platformMatchers = {
    'darwin-aarch64': [/_aarch64\.app\.tar\.gz$/i],
    'darwin-x86_64': [/_x64\.app\.tar\.gz$/i, /_x86_64\.app\.tar\.gz$/i],
    'linux-x86_64': [/\.AppImage$/i],
    'windows-x86_64': [/-setup\.exe$/i, /\.msi$/i],
  }

  const platforms = {}
  for (const [platform, patterns] of Object.entries(platformMatchers)) {
    const asset = pick(patterns)
    if (!asset) continue
    const signature = files[asset]?.trim()
    if (!signature) continue
    platforms[platform] = {
      signature,
      url: `${baseUrl}/${encodeURIComponent(asset)}`,
    }
  }

  if (Object.keys(platforms).length === 0) {
    throw new Error(`No signed updater assets matched in [${names.join(', ')}]`)
  }

  return {
    version: cleanVersion,
    notes: notes.trim(),
    pub_date: pubDate,
    platforms,
  }
}

if (import.meta.url === `file://${resolve(process.argv[1] ?? '')}`) {
  const args = parseArgs(process.argv.slice(2))
  const required = ['version', 'tag', 'repo', 'assets-dir', 'output']
  for (const key of required) {
    if (!args[key]) throw new Error(`--${key} is required`)
  }

  const dir = resolve(args['assets-dir'])
  const entries = readdirSync(dir)
  const files = {}
  for (const entry of entries) {
    if (!entry.endsWith('.sig')) continue
    const bundleName = entry.slice(0, -'.sig'.length)
    files[bundleName] = readFileSync(join(dir, entry), 'utf8').trim()
  }

  const notes = args['notes-file'] ? readFileSync(resolve(args['notes-file']), 'utf8') : (args.notes ?? '')
  const manifest = buildUpdaterManifest({
    version: args.version,
    tag: args.tag,
    repo: args.repo,
    pubDate: args['pub-date'] || new Date().toISOString(),
    notes,
    files,
  })

  writeFileSync(resolve(args.output), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8')
  console.log(
    `Generated updater manifest ${args.output} for v${manifest.version} (${Object.keys(manifest.platforms).join(', ')})`,
  )
}
