/** Domain models. All timestamps are epoch milliseconds (number). */

export type ID = string

export interface Deck {
  id: ID
  name: string
  createdAt: number
  updatedAt: number
  color: string
  schedulerKind: SchedulerKind
  settings: DeckSettings
}

export type InsertionOrder = 'sequential' | 'random'
export type LeechAction = 'tag' | 'suspend'

/** Per-deck options. The review queue honours steps, caps, order, and leeches;
 *  burying awaits a real note/template/sibling model. */
export interface DeckSettings {
  newPerDay: number
  maxReviews: number
  learnSteps: string
  insertionOrder: InsertionOrder
  relearnSteps: string
  minimumInterval: number
  leechThreshold: number
  leechAction: LeechAction
  buryRelated: boolean
  showTimer: boolean
  desiredRetention: number
  maximumInterval: number
}

export const DEFAULT_DECK_SETTINGS: DeckSettings = {
  newPerDay: 20,
  maxReviews: 200,
  learnSteps: '1m 10m',
  insertionOrder: 'sequential',
  relearnSteps: '10m',
  minimumInterval: 1,
  leechThreshold: 8,
  leechAction: 'suspend',
  buryRelated: true,
  showTimer: false,
  desiredRetention: 0.9,
  maximumInterval: 36500,
}

/** How well the user recalled a card during review. */
export type Grade = 'again' | 'hard' | 'good' | 'easy'

/** Which scheduling algorithm owns a deck's cards. FSRS is the only one today;
 *  the discriminant is kept so another algorithm stays a one-file addition. */
export type SchedulerKind = 'fsrs'

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
  /** FSRS state: 0 New / 1 Learning / 2 Review / 3 Relearning. */
  state: number
  /** Index into the deck's learn/relearn steps; 0 when state ∈ {0 New, 2 Review}. */
  step: number
  /** Last review time (epoch ms), or null if never reviewed. */
  lastReview: number | null
  /** When the card is next due (epoch ms). */
  due: number
}

/** Per-card scheduling state, owned by the deck's scheduling algorithm. */
export type SchedulingState = FSRSState

export interface Card {
  id: ID
  deckId: ID
  /** Markdown source for the question side. */
  front: string
  /** Markdown source for the answer side. */
  back: string
  createdAt: number
  updatedAt: number
  /** Durable labels. `leech` is the only system tag today. */
  tags: string[]
  /** Suspended cards stay stored/editable but do not enter due queues. */
  suspended: boolean
  /** Most recent Again grade, used by custom-study forgotten-card selection. */
  lastAgainAt: number | null
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

/** Per-deck, per-day cap counters (#3b). Local-only — not synced or backed up. */
export interface DailyStat {
  id: string          // `${deckId}:${day}`
  deckId: ID
  day: string         // local calendar date, YYYY-MM-DD
  newIntroduced: number
  reviewsDone: number
}

/** Records that a deck or card was deleted, so the deletion propagates on sync. */
export interface Tombstone {
  id: ID
  kind: 'deck' | 'card'
  /** When the deletion happened (epoch ms). */
  deletedAt: number
}
