#!/usr/bin/env node
/**
 * Builds the `rem-cli` binary and stages it at `src-tauri/binaries/rem-<target-triple>[.exe]`
 * so Tauri's `externalBin` bundles the CLI directly inside the desktop app package.
 *
 * Usage:
 *   node tools/stage-cli-sidecar.mjs [--target <triple>] [--universal]
 */
import { execFileSync } from 'node:child_process'
import { chmodSync, copyFileSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const manifestPath = join(root, 'src-tauri', 'Cargo.toml')
const binariesDir = join(root, 'src-tauri', 'binaries')

function run(cmd, args) {
  execFileSync(cmd, args, { cwd: root, stdio: 'inherit' })
}

function detectHostTriple() {
  const output = execFileSync('rustc', ['-vV'], { encoding: 'utf8' })
  const match = output.match(/^host:\s+(\S+)/m)
  if (!match) {
    throw new Error('Could not detect host target triple from `rustc -vV`')
  }
  return match[1]
}

function parseArgs(argv) {
  let target = null
  let universal = false
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    if (arg === '--universal') {
      universal = true
    } else if (arg === '--target') {
      target = argv[i + 1] ?? null
      i += 1
    } else if (arg.startsWith('--target=')) {
      target = arg.slice('--target='.length)
    } else {
      throw new Error(`Unknown argument: ${arg}`)
    }
  }
  return { target, universal }
}

const { target, universal } = parseArgs(process.argv.slice(2))
mkdirSync(binariesDir, { recursive: true })

if (universal || target === 'universal-apple-darwin') {
  const arches = ['aarch64-apple-darwin', 'x86_64-apple-darwin']
  for (const arch of arches) {
    run('cargo', ['build', '--manifest-path', manifestPath, '--release', '-p', 'rem-cli', '--target', arch])
  }
  const dest = join(binariesDir, 'rem-universal-apple-darwin')
  run('lipo', [
    '-create',
    join(root, 'src-tauri', 'target', 'aarch64-apple-darwin', 'release', 'rem'),
    join(root, 'src-tauri', 'target', 'x86_64-apple-darwin', 'release', 'rem'),
    '-output',
    dest,
  ])
  chmodSync(dest, 0o755)
  // Tauri universal builds also inspect the per-arch sidecar paths when assembling each slice.
  for (const arch of arches) {
    const archDest = join(binariesDir, `rem-${arch}`)
    copyFileSync(join(root, 'src-tauri', 'target', arch, 'release', 'rem'), archDest)
    chmodSync(archDest, 0o755)
  }
  console.log(`Staged universal rem CLI sidecar at ${dest}`)
} else {
  const resolvedTarget = target || detectHostTriple()
  const cargoArgs = ['build', '--manifest-path', manifestPath, '--release', '-p', 'rem-cli']
  if (target) {
    cargoArgs.push('--target', target)
  }
  run('cargo', cargoArgs)

  const isWindows = resolvedTarget.includes('windows')
  const ext = isWindows ? '.exe' : ''
  const builtBinary = target
    ? join(root, 'src-tauri', 'target', target, 'release', `rem${ext}`)
    : join(root, 'src-tauri', 'target', 'release', `rem${ext}`)
  const dest = join(binariesDir, `rem-${resolvedTarget}${ext}`)
  copyFileSync(builtBinary, dest)
  if (!isWindows) {
    chmodSync(dest, 0o755)
  }
  console.log(`Staged rem CLI sidecar at ${dest}`)
}
