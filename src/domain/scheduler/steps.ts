/** Split a steps string into chip tokens, e.g. "1m 10m 1d" → ["1m","10m","1d"]. */
export function parseSteps(raw: string): string[] {
  return raw.split(/\s+/).filter(Boolean)
}

const UNIT_MS: Record<string, number> = { s: 1000, m: 60_000, h: 3_600_000, d: 86_400_000 }

/** One token → milliseconds, or null if unparseable. Units s/m/h/d; a bare
 *  integer means minutes (Anki convention). */
function stepMs(token: string): number | null {
  const m = /^(\d+)([smhd]?)$/.exec(token.trim())
  if (!m) return null
  return Number(m[1]) * UNIT_MS[m[2] || 'm']
}

/** A steps string → milliseconds list, dropping unparseable tokens. */
export function parseStepsMs(raw: string): number[] {
  return parseSteps(raw)
    .map(stepMs)
    .filter((n): n is number => n !== null)
}
