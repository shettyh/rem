#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'

// Unambiguous base32-like alphabet (no 0/O/1/I/L/U)
const ALPHABET = '23456789ABCDEFGHJKMNPQRSTVWXYZ'
const DEFAULT_DB = 'rem-sync-auth'
const DAY_MS = 86_400_000

function generateCode() {
  const chars = []
  const limit = Math.floor(256 / ALPHABET.length) * ALPHABET.length
  while (chars.length < 8) {
    const buf = randomBytes(16)
    for (const byte of buf) {
      if (byte < limit) {
        chars.push(ALPHABET[byte % ALPHABET.length])
        if (chars.length === 8) break
      }
    }
  }
  return `REM-${chars.slice(0, 4).join('')}-${chars.slice(4, 8).join('')}`
}

function sqlString(value) {
  if (value === null || value === undefined) return 'NULL'
  return `'${String(value).replace(/'/g, "''")}'`
}

function parsePositiveInt(raw, flagName) {
  const parsed = Number.parseInt(raw, 10)
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${flagName} must be a positive integer (got "${raw}")`)
  }
  return parsed
}

function usage() {
  console.error(`Usage:
  node scripts/invites.mjs create [--count 1] [--uses 1] [--days 365 | --forever] [--expires-days 30] [--plan invite_free] [--note "..."] [--db ${DEFAULT_DB}] [--remote] [--dry-run]
  node scripts/invites.mjs list [--db ${DEFAULT_DB}] [--remote]
  node scripts/invites.mjs revoke <code> [--db ${DEFAULT_DB}] [--remote] [--dry-run]`)
}

function runD1(sql, { db, remote, dryRun }) {
  if (dryRun) {
    console.log(sql)
    return
  }
  const args = ['wrangler', 'd1', 'execute', db, remote ? '--remote' : '--local', '--command', sql]
  execFileSync('npx', args, { stdio: 'inherit' })
}

function parseFlags(argv) {
  const opts = {
    count: 1,
    uses: 1,
    days: null,
    forever: false,
    expiresDays: null,
    plan: 'invite_free',
    note: null,
    db: DEFAULT_DB,
    remote: false,
    dryRun: false,
    positionals: [],
  }

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (arg === '--remote') {
      opts.remote = true
    } else if (arg === '--local') {
      opts.remote = false
    } else if (arg === '--dry-run') {
      opts.dryRun = true
    } else if (arg === '--forever') {
      opts.forever = true
      opts.days = null
    } else if (arg === '--count') {
      opts.count = parsePositiveInt(argv[++i], '--count')
    } else if (arg === '--uses') {
      opts.uses = parsePositiveInt(argv[++i], '--uses')
    } else if (arg === '--days') {
      opts.days = parsePositiveInt(argv[++i], '--days')
      opts.forever = false
    } else if (arg === '--expires-days') {
      opts.expiresDays = parsePositiveInt(argv[++i], '--expires-days')
    } else if (arg === '--plan') {
      opts.plan = argv[++i] ?? 'invite_free'
    } else if (arg === '--note') {
      opts.note = argv[++i] ?? ''
    } else if (arg === '--db') {
      opts.db = argv[++i] ?? DEFAULT_DB
    } else if (arg.startsWith('-')) {
      throw new Error(`Unknown flag: ${arg}`)
    } else {
      opts.positionals.push(arg)
    }
  }
  return opts
}

function main() {
  const [command, ...rest] = process.argv.slice(2)
  if (!command || command === '-h' || command === '--help') {
    usage()
    process.exit(command ? 0 : 1)
  }

  const opts = parseFlags(rest)
  const now = Date.now()

  if (command === 'create') {
    const grantDays = opts.forever ? null : opts.days
    const expiresAt = opts.expiresDays !== null ? now + opts.expiresDays * DAY_MS : null
    const codes = Array.from({ length: opts.count }, () => generateCode())

    const statements = codes.map(
      (code) =>
        `INSERT INTO invite_codes (code, max_uses, uses_count, grant_days, plan_label, note, expires_at, revoked_at, created_at) VALUES (${sqlString(code)}, ${opts.uses}, 0, ${grantDays === null ? 'NULL' : grantDays}, ${sqlString(opts.plan)}, ${sqlString(opts.note)}, ${expiresAt === null ? 'NULL' : expiresAt}, NULL, ${now});`,
    )
    runD1(statements.join('\n'), opts)
    console.log('\nCreated invite code(s):')
    for (const code of codes) {
      const duration = grantDays === null ? 'forever' : `${grantDays}d`
      console.log(`  ${code}  (uses=${opts.uses}, grant=${duration})`)
    }
    return
  }

  if (command === 'list') {
    const sql =
      "SELECT code, uses_count || '/' || max_uses AS uses, COALESCE(CAST(grant_days AS TEXT) || 'd', 'forever') AS grant_length, plan_label, note, CASE WHEN revoked_at IS NOT NULL THEN 'revoked' WHEN expires_at IS NOT NULL AND expires_at <= unixepoch()*1000 THEN 'expired' WHEN uses_count >= max_uses THEN 'exhausted' ELSE 'active' END AS state, datetime(created_at/1000, 'unixepoch') AS created_at FROM invite_codes ORDER BY created_at DESC LIMIT 100;"
    runD1(sql, opts)
    return
  }

  if (command === 'revoke') {
    const rawCode = opts.positionals[0]
    if (!rawCode) {
      throw new Error('Missing invite code to revoke.')
    }
    const code = rawCode.trim().toUpperCase()
    const sql = `UPDATE invite_codes SET revoked_at = ${now} WHERE code = ${sqlString(code)} AND revoked_at IS NULL;`
    runD1(sql, opts)
    console.log(`Revoked invite code: ${code}`)
    return
  }

  throw new Error(`Unknown command: ${command}`)
}

try {
  main()
} catch (err) {
  console.error(err instanceof Error ? err.message : String(err))
  usage()
  process.exit(1)
}
