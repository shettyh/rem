import type { DeckSettings, Grade, ID, SchedulerKind, SchedulingState } from '../domain/models'
import { DEFAULT_DECK_SETTINGS } from '../domain/models'
import { getScheduler } from '../domain/scheduler'
import type { Storage } from './Storage'

export interface ReviewBackup {
  reviewedAt: number
  grade: Grade
}

export interface CardBackup {
  front: string
  back: string
  createdAt: number
  updatedAt: number
  tags: string[]
  suspended: boolean
  lastAgainAt: number | null
  scheduling: SchedulingState
  reviews: ReviewBackup[]
}

export interface DeckBackup {
  name: string
  createdAt: number
  schedulerKind: SchedulerKind
  color?: string
  settings: DeckSettings
  cards: CardBackup[]
}

export interface BackupFile {
  format: 'rem-backup'
  version: 1
  exportedAt: number
  decks: DeckBackup[]
}

/** Read the named decks (with their cards) into the DB-agnostic backup shape. */
export async function collectBackup(storage: Storage, deckIds: ID[]): Promise<DeckBackup[]> {
  const decks = await storage.listDecks()
  const byId = new Map(decks.map((d) => [d.id, d]))
  const out: DeckBackup[] = []
  for (const id of deckIds) {
    const deck = byId.get(id)
    if (!deck) continue
    const [cards, logs] = await Promise.all([
      storage.listCards(id),
      storage.listReviewLogs(id),
    ])
    const reviewsByCard = new Map<ID, ReviewBackup[]>()
    for (const log of logs) {
      const reviews = reviewsByCard.get(log.cardId) ?? []
      reviews.push({ reviewedAt: log.reviewedAt, grade: log.grade })
      reviewsByCard.set(log.cardId, reviews)
    }
    out.push({
      name: deck.name,
      createdAt: deck.createdAt,
      schedulerKind: deck.schedulerKind,
      color: deck.color,
      settings: deck.settings,
      cards: cards.map((c) => ({
        front: c.front,
        back: c.back,
        createdAt: c.createdAt,
        updatedAt: c.updatedAt,
        tags: c.tags,
        suspended: c.suspended,
        lastAgainAt: c.lastAgainAt,
        scheduling: c.scheduling,
        reviews: reviewsByCard.get(c.id) ?? [],
      })),
    })
  }
  return out
}

export function serializeBackup(decks: DeckBackup[], exportedAt: number): string {
  const file: BackupFile = { format: 'rem-backup', version: 1, exportedAt, decks }
  return JSON.stringify(file, null, 2)
}

export function parseBackup(text: string, now: number): DeckBackup[] {
  let data: unknown
  try {
    data = JSON.parse(text)
  } catch {
    throw new Error('Not a valid JSON file.')
  }
  if (!isObject(data) || data.format !== 'rem-backup') {
    throw new Error('Not a rem backup file.')
  }
  if (data.version !== 1) {
    throw new Error(`Unsupported backup version: ${String(data.version)}.`)
  }
  if (!Array.isArray(data.decks)) {
    throw new Error('Backup file is malformed.')
  }
  return data.decks.map((d) => parseDeck(d, now))
}

/** Classify incoming deck names against existing names (de-duplicated). */
export function planImport(
  incomingNames: string[],
  existingNames: string[],
): { added: string[]; replaced: string[] } {
  const existing = new Set(existingNames)
  const seen = new Set<string>()
  const added: string[] = []
  const replaced: string[] = []
  for (const name of incomingNames) {
    if (seen.has(name)) continue
    seen.add(name)
    if (existing.has(name)) replaced.push(name)
    else added.push(name)
  }
  return { added, replaced }
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

function parseDeck(raw: unknown, now: number): DeckBackup {
  if (
    !isObject(raw) ||
    typeof raw.name !== 'string' ||
    typeof raw.createdAt !== 'number' ||
    !Array.isArray(raw.cards)
  ) {
    throw new Error('Backup file is malformed.')
  }
  return {
    name: raw.name,
    createdAt: raw.createdAt,
    schedulerKind: 'fsrs',
    color: typeof raw.color === 'string' ? raw.color : undefined,
    settings: { ...DEFAULT_DECK_SETTINGS, ...(isObject(raw.settings) ? raw.settings : {}) },
    cards: raw.cards.map((c) => parseCard(c, now)),
  }
}

function parseCard(raw: unknown, now: number): CardBackup {
  if (
    !isObject(raw) ||
    typeof raw.front !== 'string' ||
    typeof raw.back !== 'string' ||
    typeof raw.createdAt !== 'number' ||
    typeof raw.updatedAt !== 'number' ||
    !isSchedulingPayload(raw.scheduling)
  ) {
    throw new Error('Backup file is malformed.')
  }
  return {
    front: raw.front,
    back: raw.back,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    tags: Array.isArray(raw.tags) && raw.tags.every((tag) => typeof tag === 'string') ? raw.tags : [],
    suspended: raw.suspended === true,
    lastAgainAt: typeof raw.lastAgainAt === 'number' ? raw.lastAgainAt : null,
    scheduling: normalizeScheduling(raw.scheduling, now),
    reviews: parseReviews(raw.reviews),
  }
}

function parseReviews(raw: unknown): ReviewBackup[] {
  if (raw === undefined) return []
  if (!Array.isArray(raw)) throw new Error('Backup file is malformed.')
  return raw.map((review) => {
    if (
      !isObject(review) ||
      typeof review.reviewedAt !== 'number' ||
      !isGrade(review.grade)
    ) {
      throw new Error('Backup file is malformed.')
    }
    return { reviewedAt: review.reviewedAt, grade: review.grade }
  })
}

function isGrade(value: unknown): value is Grade {
  return value === 'again' || value === 'hard' || value === 'good' || value === 'easy'
}

/** Whether `v` is a recognizable scheduling payload — FSRS, or a legacy SM-2
 *  shape (kind 'sm2' or pre-discriminant). SM-2 payloads are accepted only so
 *  old backups parse; they get reset to FSRS in {@link normalizeScheduling}. */
function isSchedulingPayload(v: unknown): v is Record<string, unknown> {
  if (!isObject(v) || typeof v.due !== 'number') return false
  if (v.kind === 'fsrs') {
    return (
      typeof v.stability === 'number' &&
      typeof v.difficulty === 'number' &&
      typeof v.reps === 'number' &&
      typeof v.lapses === 'number' &&
      typeof v.state === 'number' &&
      (v.lastReview === null || typeof v.lastReview === 'number')
    )
  }
  // legacy SM-2 (kind 'sm2' or absent)
  return (
    typeof v.repetitions === 'number' &&
    typeof v.intervalDays === 'number' &&
    typeof v.easeFactor === 'number'
  )
}

/** FSRS state passes through; any legacy SM-2 state is reset to a fresh FSRS
 *  card (due `now`), since SM-2 is no longer supported. */
function normalizeScheduling(v: Record<string, unknown>, now: number): SchedulingState {
  // step may be absent in backups written before #3a (JSON is cast, not validated);
  // spread order means an existing v.step wins, a missing one defaults to 0.
  if (v.kind === 'fsrs') return { step: 0, ...v } as unknown as SchedulingState
  return getScheduler().initial(now)
}
