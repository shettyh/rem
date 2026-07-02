import { describe, it, expect } from 'vitest'
import type { Card, DeckSettings, FSRSState } from '../../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../../domain/models'
import { ReviewSession, buildSessionCards, LEARN_AHEAD_MS, type SessionCard } from './session'

const S: DeckSettings = DEFAULT_DECK_SETTINGS
function sched(state: number, due: number): FSRSState {
  return { kind: 'fsrs', stability: 0, difficulty: 0, reps: state === 0 ? 0 : 1, lapses: 0, state, step: 0, lastReview: null, due }
}
function card(id: string, createdAt: number, s: FSRSState): SessionCard {
  return { card: { id, deckId: 'd', front: id, back: id, createdAt, updatedAt: createdAt, scheduling: s } as Card, settings: S }
}

describe('buildSessionCards', () => {
  it('sequential: reviews by due, then new by createdAt', () => {
    const cards = [
      card('n2', 20, sched(0, 0)),
      card('r1', 5, sched(2, 100)),
      card('n1', 10, sched(0, 0)),
    ]
    const out = buildSessionCards(cards, 'sequential').map((c) => c.card.id)
    expect(out).toEqual(['r1', 'n1', 'n2'])
  })
  it('random: preserves the full set of new cards', () => {
    const cards = [card('n1', 10, sched(0, 0)), card('n2', 20, sched(0, 0)), card('n3', 30, sched(0, 0))]
    const out = buildSessionCards(cards, 'random').map((c) => c.card.id).sort()
    expect(out).toEqual(['n1', 'n2', 'n3'])
  })
  it('sequential: orders review cards by due ascending, regardless of array order', () => {
    const cards = [card('r2', 1, sched(2, 200)), card('r1', 2, sched(2, 100))]
    const out = buildSessionCards(cards, 'sequential').map((c) => c.card.id)
    expect(out).toEqual(['r1', 'r2'])
  })
})

describe('ReviewSession', () => {
  it('serves due cards in order and counts reviewed/remaining', () => {
    const now = 1000
    const s = new ReviewSession([card('a', 1, sched(2, 0)), card('b', 2, sched(2, 0))])
    expect(s.remaining).toBe(2)
    expect(s.next(now)!.card.id).toBe('a')
    s.grade(now, sched(2, now + 5 * 86_400_000)) // graduated far out → leaves session
    expect(s.reviewed).toBe(1)
    expect(s.next(now)!.card.id).toBe('b')
  })

  it('re-inserts a short-step card and serves it early via learn-ahead', () => {
    const now = 1000
    const s = new ReviewSession([card('a', 1, sched(0, 0)), card('b', 2, sched(2, 0))])
    expect(s.next(now)!.card.id).toBe('a')
    s.grade(now, sched(1, now + 600_000)) // 'a' → learning, due in 10m (within window)
    // 'b' is due now, so it comes next; 'a' is not yet due
    expect(s.next(now)!.card.id).toBe('b')
    s.grade(now, sched(2, now + 5 * 86_400_000))
    // only 'a' left, still 10m out but within learn-ahead → served early
    expect(s.next(now)!.card.id).toBe('a')
  })

  it('reappears once genuinely due after re-insertion (due <= now)', () => {
    const now = 1000
    const s = new ReviewSession([card('a', 1, sched(0, 0)), card('b', 2, sched(2, 0))])
    expect(s.next(now)!.card.id).toBe('a')
    s.grade(now, sched(1, now + 600_000)) // 'a' → learning, due in 10m (within window) → re-inserted
    expect(s.next(now)!.card.id).toBe('b') // 'b' due now
    s.grade(now, sched(2, now + 5 * 86_400_000)) // 'b' graduates far out, leaves session
    const later = now + 600_000 + 1
    expect(s.next(later)!.card.id).toBe('a') // now genuinely due, not just learn-ahead
    expect(s.reviewed).toBe(2)
  })

  it('learn-ahead: does not serve a step card beyond the window', () => {
    const now = 1000
    const s = new ReviewSession([card('a', 1, sched(1, now + LEARN_AHEAD_MS + 60_000))])
    expect(s.next(now)).toBeNull()
  })

  it('learn-ahead window boundary: served exactly at the edge, null one ms beyond', () => {
    const now = 1000
    const atEdge = new ReviewSession([card('a', 1, sched(1, now + LEARN_AHEAD_MS))])
    expect(atEdge.next(now)!.card.id).toBe('a')
    const beyondEdge = new ReviewSession([card('b', 1, sched(1, now + LEARN_AHEAD_MS + 1))])
    expect(beyondEdge.next(now)).toBeNull()
  })

  it('learn-ahead picks the earliest-due card, not the queue head', () => {
    const now = 1000
    const s = new ReviewSession([
      card('a', 1, sched(2, now + 15 * 60_000)),
      card('b', 2, sched(2, now + 5 * 60_000)),
    ])
    expect(s.next(now)!.card.id).toBe('b')
  })

  it('empty session → null', () => {
    expect(new ReviewSession([]).next(1000)).toBeNull()
  })
})
