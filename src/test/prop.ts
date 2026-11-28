/** Deterministic zero-dependency PRNG and async helpers for property/interleaving tests. */

export type Rng = () => number

/** Mulberry32 deterministic PRNG producing floats in [0, 1). */
export function makeRng(seed: number): Rng {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6d2b79f5) >>> 0
    let t = Math.imul(state ^ (state >>> 15), 1 | state)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export function randInt(rng: Rng, min: number, max: number): number {
  return min + Math.floor(rng() * (max - min + 1))
}

export function randBool(rng: Rng, probability = 0.5): boolean {
  return rng() < probability
}

export function pick<T>(rng: Rng, items: readonly T[]): T {
  return items[Math.floor(rng() * items.length)]
}

export function randString(rng: Rng, alphabet: string, minLen: number, maxLen: number): string {
  const len = randInt(rng, minLen, maxLen)
  let out = ''
  for (let i = 0; i < len; i++) {
    out += alphabet[Math.floor(rng() * alphabet.length)]
  }
  return out
}

/** Run `body` `runs` times with a fixed seed; rethrow failures annotated with seed & iteration. */
export function forAll(runs: number, seed: number, body: (rng: Rng, iter: number) => void): void {
  const rng = makeRng(seed)
  for (let iter = 0; iter < runs; iter++) {
    try {
      body(rng, iter)
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err)
      throw new Error(`Property failed [seed=${seed}, iter=${iter}]: ${detail}`, { cause: err })
    }
  }
}

/** Async version of {@link forAll}. */
export async function forAllAsync(
  runs: number,
  seed: number,
  body: (rng: Rng, iter: number) => Promise<void>,
): Promise<void> {
  const rng = makeRng(seed)
  for (let iter = 0; iter < runs; iter++) {
    try {
      await body(rng, iter)
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err)
      throw new Error(`Async property failed [seed=${seed}, iter=${iter}]: ${detail}`, { cause: err })
    }
  }
}

export interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T | PromiseLike<T>) => void
  reject: (reason?: unknown) => void
}

/** Controllable promise for deterministic async-interleaving tests. */
export function deferred<T>(): Deferred<T> {
  let resolve!: (value: T | PromiseLike<T>) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}
