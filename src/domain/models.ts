/** Domain models. All timestamps are epoch milliseconds (number). */

export type ID = string

export interface Deck {
  id: ID
  name: string
  createdAt: number
}

/** How well the user recalled a card during review. */
export type Grade = 'again' | 'hard' | 'good' | 'easy'

/** Per-card scheduling state owned by the scheduling algorithm. */
export interface SchedulingState {
  /** Number of consecutive successful reviews. */
  repetitions: number
  /** Current inter-review interval in days. */
  intervalDays: number
  /** SM-2 ease factor (>= 1.3). */
  easeFactor: number
  /** When the card is next due (epoch ms). */
  due: number
}

export interface Card {
  id: ID
  deckId: ID
  /** Markdown source for the question side. */
  front: string
  /** Markdown source for the answer side. */
  back: string
  createdAt: number
  updatedAt: number
  scheduling: SchedulingState
}
