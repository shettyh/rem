/** Domain models. All timestamps are epoch milliseconds (number). */

export type ID = string

export interface Deck {
  id: ID
  name: string
  createdAt: number
  /** Which scheduling algorithm this deck's cards use (fixed at creation). */
  schedulerKind: SchedulerKind
}

/** How well the user recalled a card during review. */
export type Grade = 'again' | 'hard' | 'good' | 'easy'

/** Which scheduling algorithm owns a deck's cards. */
export type SchedulerKind = 'sm2' | 'fsrs'

/** SM-2 per-card scheduling state. */
export interface SM2State {
  kind: 'sm2'
  /** Number of consecutive successful reviews. */
  repetitions: number
  /** Current inter-review interval in days. */
  intervalDays: number
  /** SM-2 ease factor (>= 1.3). */
  easeFactor: number
  /** When the card is next due (epoch ms). */
  due: number
}

/** FSRS per-card scheduling state. */
export interface FSRSState {
  kind: 'fsrs'
  /** Memory stability in days. */
  stability: number
  /** Card difficulty (1–10). */
  difficulty: number
  /** Total reviews so far. */
  reps: number
  /** Number of failed reviews. */
  lapses: number
  /** ts-fsrs State enum: 0 New / 1 Learning / 2 Review / 3 Relearning. */
  state: number
  /** Last review time (epoch ms), or null if never reviewed. */
  lastReview: number | null
  /** When the card is next due (epoch ms). */
  due: number
}

/** Per-card scheduling state, owned by the deck's scheduling algorithm. */
export type SchedulingState = SM2State | FSRSState

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

/** A content-addressed binary asset (image/GIF) embedded in card markdown as `asset:<hash>`. */
export interface Asset {
  /** SHA-256 hex of {@link bytes}; primary key. */
  hash: ID
  /** MIME type, e.g. image/png. */
  mime: string
  bytes: Uint8Array
  createdAt: number
}

/** Records that a deck or card was deleted, so the deletion propagates on sync. */
export interface Tombstone {
  id: ID
  kind: 'deck' | 'card'
  /** When the deletion happened (epoch ms). */
  deletedAt: number
}
