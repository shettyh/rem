#!/usr/bin/env node
/**
 * Render an Xcode ExportOptions.plist for App Store Connect / TestFlight
 * cloud-signed uploads (no local .p12 distribution certificate required).
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const TEAM_ID_RE = /^[A-Z0-9]{10}$/
const ALLOWED_DESTINATIONS = new Set(['upload', 'export'])

function parseArgs(argv) {
  const args = {}
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i]
    if (!token.startsWith('--')) continue
    const key = token.slice(2)
    const value = argv[i + 1]
    if (value === undefined || value.startsWith('--')) {
      args[key] = true
    } else {
      args[key] = value
      i += 1
    }
  }
  return args
}

function parseBool(value, defaultValue = false) {
  if (value === undefined) return defaultValue
  if (typeof value === 'boolean') return value
  const normalized = String(value).trim().toLowerCase()
  if (normalized === 'true' || normalized === '1' || normalized === 'yes') return true
  if (normalized === 'false' || normalized === '0' || normalized === 'no' || normalized === '') {
    return false
  }
  throw new Error(`Invalid boolean value: ${value}`)
}

export function renderIosExportOptions({
  teamId,
  destination = 'upload',
  internalOnly = false,
  manageAppVersionAndBuildNumber = true,
  uploadSymbols = true,
}) {
  const cleanTeamId = String(teamId ?? '').trim().toUpperCase()
  if (!TEAM_ID_RE.test(cleanTeamId)) {
    throw new Error(`Invalid Apple Team ID (expected 10 alphanumeric characters): ${teamId}`)
  }

  const cleanDestination = String(destination ?? '').trim().toLowerCase()
  if (!ALLOWED_DESTINATIONS.has(cleanDestination)) {
    throw new Error(
      `Invalid export destination (expected upload or export): ${destination}`,
    )
  }

  const entries = [
    ['destination', `<string>${cleanDestination}</string>`],
    ['manageAppVersionAndBuildNumber', manageAppVersionAndBuildNumber ? '<true/>' : '<false/>'],
    ['method', '<string>app-store-connect</string>'],
    ['signingStyle', '<string>automatic</string>'],
    ['stripSwiftSymbols', '<true/>'],
    ['teamID', `<string>${cleanTeamId}</string>`],
  ]

  if (internalOnly) {
    entries.push(['testFlightInternalTestingOnly', '<true/>'])
  }

  entries.push(['uploadSymbols', uploadSymbols ? '<true/>' : '<false/>'])

  const body = entries
    .map(([key, value]) => `\t<key>${key}</key>\n\t${value}`)
    .join('\n')

  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
${body}
</dict>
</plist>
`
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)

if (isMain) {
  const args = parseArgs(process.argv.slice(2))
  if (!args['team-id']) {
    console.error(
      'Usage: node tools/render-ios-export-options.mjs --team-id <TEAM_ID> [--destination upload|export] [--internal-only true|false] [--output ExportOptions.plist]',
    )
    process.exit(1)
  }

  const plist = renderIosExportOptions({
    teamId: args['team-id'],
    destination: args.destination ?? 'upload',
    internalOnly: parseBool(args['internal-only'], false),
  })

  if (args.output) {
    fs.mkdirSync(path.dirname(args.output), { recursive: true })
    fs.writeFileSync(args.output, plist, 'utf8')
    console.log(`Wrote iOS ExportOptions.plist to ${args.output}`)
  } else {
    process.stdout.write(plist)
  }
}
