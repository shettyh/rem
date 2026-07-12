import type { Card, DeckSettings, FSRSState, InsertionOrder } from '../../domain/models'
import { shuffle } from './dueOverview'

/** How early a not-yet-due learning card may be shown so the user never waits. */
export const LEARN_AHEAD_MS = 20 * 60_000

export interface SessionCard {
  card: Card
  settings: DeckSettings
}

export interface Caps {
  newSlots: number
  reviewSlots: number
}

/** Initial single-deck order: in-progress + due reviews first (by due), then new
 *  cards in insertion order. `caps` bounds how many New (state 0) and Review
 *  (state 2) cards enter; learning/relearning (state 1/3) are never capped. */
export function buildSessionCards(
  cards: SessionCard[],
  order: InsertionOrder,
  caps: Caps = { newSlots: Infinity, reviewSlots: Infinity },
): SessionCard[] {
  const newSlots = Math.max(0, caps.newSlots)
  const reviewSlots = Math.max(0, caps.reviewSlots)

  const news = cards.filter((c) => c.card.scheduling.state === 0)
  const reviews = cards.filter((c) => c.card.scheduling.state === 2)
  const inProgress = cards.filter(
    (c) => c.card.scheduling.state === 1 || c.card.scheduling.state === 3,
  )

  const orderedNew = (
    order === 'random'
      ? shuffle(news)
      : news.slice().sort((a, b) => a.card.createdAt - b.card.createdAt)
  ).slice(0, newSlots)

  const keptReview = reviews
    .slice()
    .sort((a, b) => a.card.scheduling.due - b.card.scheduling.due)
    .slice(0, reviewSlots)

  const orderedRest = [...inProgress, ...keptReview].sort(
    (a, b) => a.card.scheduling.due - b.card.scheduling.due,
  )

  return [...orderedRest, ...orderedNew]
}

/** One review sitting. Holds an ordered working queue; step cards re-insert and
 *  are shown when due, or a little early via learn-ahead. Pure — no storage. */
export class ReviewSession {
  private queue: SessionCard[]
  private current: SessionCard | null = null
  private _reviewed = 0

  constructor(cards: SessionCard[]) {
    this.queue = cards.slice()
  }

  get remaining(): number {
    return this.queue.length + (this.current ? 1 : 0)
  }

  get reviewed(): number {
    return this._reviewed
  }

  next(now: number): SessionCard | null {
    // First card already due, in queue order.
    let pick = this.queue.findIndex((c) => c.card.scheduling.due <= now)
    if (pick < 0) {
      // Nothing due — learn-ahead: the earliest-due card, if within the window.
      if (this.queue.length === 0) {
        this.current = null
        return null
      }
      let earliest = 0
      for (let k = 1; k < this.queue.length; k++) {
        if (this.queue[k].card.scheduling.due < this.queue[earliest].card.scheduling.due) earliest = k
      }
      if (this.queue[earliest].card.scheduling.due - now > LEARN_AHEAD_MS) {
        this.current = null
        return null
      }
      pick = earliest
    }
    const [chosen] = this.queue.splice(pick, 1)
    this.current = chosen
    return chosen
  }

  grade(now: number, next: FSRSState): void {
    const cur = this.current
    if (!cur) return
    this._reviewed += 1
    this.current = null
    const stillStepping = (next.state === 1 || next.state === 3) && next.due - now <= LEARN_AHEAD_MS
    if (stillStepping) {
      this.queue.push({ ...cur, card: { ...cur.card, scheduling: next } })
    }
  }
}
