#!/usr/bin/env node
/**
 * Renders the Homebrew Cask definition (`Casks/rem.rb`) for `shettyh/homebrew-tap`.
 * Installing the cask installs both `/Applications/rem.app` and links the bundled
 * `rem` CLI binary onto Homebrew's PATH in a single command.
 *
 * Usage:
 *   node tools/render-homebrew-cask.mjs \
 *     --version 0.19.0 \
 *     --arm-sha256 <64-hex> \
 *     --intel-sha256 <64-hex> \
 *     [--output Casks/rem.rb]
 */
import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const SHA256_RE = /^[a-f0-9]{64}$/i

export function renderHomebrewCask({ version, armSha256, intelSha256, repo = 'shettyh/rem' }) {
  const cleanVersion = version.replace(/^v/, '')
  if (!/^\d+\.\d+\.\d+([-.][0-9A-Za-z.]+)?$/.test(cleanVersion)) {
    throw new Error(`Invalid semantic version: ${version}`)
  }
  if (!SHA256_RE.test(armSha256)) {
    throw new Error(`Invalid arm64 SHA-256 digest: ${armSha256}`)
  }
  if (!SHA256_RE.test(intelSha256)) {
    throw new Error(`Invalid intel SHA-256 digest: ${intelSha256}`)
  }

  return `cask "rem" do
  arch arm: "aarch64", intel: "x64"

  version "${cleanVersion}"
  sha256 arm:   "${armSha256.toLowerCase()}",
         intel: "${intelSha256.toLowerCase()}"

  url "https://github.com/${repo}/releases/download/v#{version}/rem_#{version}_#{arch}.dmg",
      verified: "github.com/${repo}/"
  name "rem"
  desc "Local-first spaced repetition across desktop, terminal, and iPhone"
  homepage "https://shettyh.github.io/rem/"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on macos: ">= :ventura"

  app "rem.app"
  binary "#{appdir}/rem.app/Contents/MacOS/rem"

  zap trash: [
    "~/Library/Application Support/com.shettyh.rem",
    "~/Library/Caches/com.shettyh.rem",
    "~/Library/Preferences/com.shettyh.rem.plist",
    "~/Library/Saved Application State/com.shettyh.rem.savedState",
  ]
end
`
}

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

if (import.meta.url === `file://${resolve(process.argv[1] ?? '')}`) {
  const args = parseArgs(process.argv.slice(2))
  const content = renderHomebrewCask({
    version: args.version ?? '',
    armSha256: args['arm-sha256'] ?? '',
    intelSha256: args['intel-sha256'] ?? '',
    repo: args.repo || 'shettyh/rem',
  })

  if (args.output) {
    writeFileSync(resolve(args.output), content, 'utf8')
    console.log(`Wrote Homebrew cask to ${args.output}`)
  } else {
    process.stdout.write(content)
  }
}
