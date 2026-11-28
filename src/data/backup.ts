import type { DeckSettings, DraftSource, FSRSState, Grade, ID, SchedulerKind, SchedulingState } from '../domain/models'
import { DEFAULT_DECK_SETTINGS, initialFsrsState } from '../domain/models'
import { normalizeDeckName } from '../features/decks/deckTree'
import type { Storage } from './Storage'

const MAX_VALID_TIMESTAMP_MS = 8_640_000_000_000_000
const MAX_U32 = 4_294_967_295
const MAX_INTERVAL_DAYS = 36_500

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
  rationale?: string | null
  sources?: DraftSource[]
  proposedBy?: string | null
  siblingGroupId?: ID | null
  clozeIndex?: number | null
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
        ...(c.rationale ? { rationale: c.rationale } : {}),
        ...(c.sources && c.sources.length > 0 ? { sources: c.sources } : {}),
        ...(c.proposedBy ? { proposedBy: c.proposedBy } : {}),
        ...(c.siblingGroupId ? { siblingGroupId: c.siblingGroupId } : {}),
        ...(c.clozeIndex ? { clozeIndex: c.clozeIndex } : {}),
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
  const seenNames = new Set<string>()
  const decks: DeckBackup[] = []
  for (const rawDeck of data.decks) {
    const deck = parseDeck(rawDeck, now)
    if (seenNames.has(deck.name)) {
      throw new Error(`Backup contains duplicate deck name: ${deck.name}.`)
    }
    seenNames.add(deck.name)
    decks.push(deck)
  }
  return decks
}

function normalizeNameForPlan(raw: string): string {
  try {
    return normalizeDeckName(raw)
  } catch {
    return raw.trim()
  }
}

/** Classify incoming deck names against existing names (normalized & de-duplicated). */
export function planImport(
  incomingNames: string[],
  existingNames: string[],
): { added: string[]; replaced: string[] } {
  const existing = new Set(existingNames.map(normalizeNameForPlan))
  const seen = new Set<string>()
  const added: string[] = []
  const replaced: string[] = []
  for (const rawName of incomingNames) {
    const name = normalizeNameForPlan(rawName)
    if (seen.has(name)) continue
    seen.add(name)
    if (existing.has(name)) replaced.push(name)
    else added.push(name)
  }
  return { added, replaced }
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}

function isValidTimestamp(v: unknown): v is number {
  return typeof v === 'number' && Number.isFinite(v) && Math.abs(v) <= MAX_VALID_TIMESTAMP_MS
}

function isNonNegativeInt(v: unknown, max = MAX_U32): v is number {
  return typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= max
}

function isFiniteNonNegative(v: unknown): v is number {
  return typeof v === 'number' && Number.isFinite(v) && v >= 0
}

function parseDeckSettings(raw: unknown): DeckSettings {
  const settings: DeckSettings = { ...DEFAULT_DECK_SETTINGS }
  if (!isObject(raw)) return settings
  if (isNonNegativeInt(raw.newPerDay, Number.MAX_SAFE_INTEGER)) {
    settings.newPerDay = Math.min(9_999, raw.newPerDay)
  }
  if (isNonNegativeInt(raw.maxReviews, Number.MAX_SAFE_INTEGER)) {
    settings.maxReviews = Math.min(9_999, raw.maxReviews)
  }
  if (typeof raw.learnSteps === 'string') {
    settings.learnSteps = raw.learnSteps.trim()
  }
  if (raw.insertionOrder === 'random' || raw.insertionOrder === 'sequential') {
    settings.insertionOrder = raw.insertionOrder
  }
  if (typeof raw.relearnSteps === 'string') {
    settings.relearnSteps = raw.relearnSteps.trim()
  }
  if (typeof raw.minimumInterval === 'number' && Number.isInteger(raw.minimumInterval) && raw.minimumInterval >= 1 && raw.minimumInterval <= 365) {
    settings.minimumInterval = raw.minimumInterval
  }
  if (typeof raw.leechThreshold === 'number' && Number.isInteger(raw.leechThreshold) && raw.leechThreshold >= 1 && raw.leechThreshold <= 99) {
    settings.leechThreshold = raw.leechThreshold
  }
  if (raw.leechAction === 'suspend' || raw.leechAction === 'tag') {
    settings.leechAction = raw.leechAction
  }
  if (typeof raw.buryRelated === 'boolean') {
    settings.buryRelated = raw.buryRelated
  }
  if (typeof raw.showTimer === 'boolean') {
    settings.showTimer = raw.showTimer
  }
  if (typeof raw.desiredRetention === 'number' && Number.isFinite(raw.desiredRetention) && raw.desiredRetention >= 0.7 && raw.desiredRetention <= 0.99) {
    settings.desiredRetention = raw.desiredRetention
  }
  if (typeof raw.maximumInterval === 'number' && Number.isInteger(raw.maximumInterval) && raw.maximumInterval >= 1) {
    settings.maximumInterval = Math.min(MAX_INTERVAL_DAYS, raw.maximumInterval)
  }
  if (raw.fsrsWeights === null) {
    settings.fsrsWeights = null
  } else if (
    Array.isArray(raw.fsrsWeights) &&
    raw.fsrsWeights.length > 0 &&
    raw.fsrsWeights.every((n) => typeof n === 'number' && Number.isFinite(n))
  ) {
    settings.fsrsWeights = raw.fsrsWeights.slice()
  }
  return settings
}

function parseDeck(raw: unknown, now: number): DeckBackup {
  if (
    !isObject(raw) ||
    typeof raw.name !== 'string' ||
    !isValidTimestamp(raw.createdAt) ||
    !Array.isArray(raw.cards)
  ) {
    throw new Error('Backup file is malformed.')
  }
  const name = normalizeDeckName(raw.name)
  const color = typeof raw.color === 'string' && raw.color.trim() ? raw.color.trim() : undefined
  return {
    name,
    createdAt: raw.createdAt,
    schedulerKind: 'fsrs',
    color,
    settings: parseDeckSettings(raw.settings),
    cards: raw.cards.map((c) => parseCard(c, now)),
  }
}

function parseCard(raw: unknown, now: number): CardBackup {
  if (
    !isObject(raw) ||
    typeof raw.front !== 'string' ||
    !raw.front.trim() ||
    typeof raw.back !== 'string' ||
    !isValidTimestamp(raw.createdAt) ||
    !isValidTimestamp(raw.updatedAt) ||
    !isSchedulingPayload(raw.scheduling)
  ) {
    throw new Error('Backup file is malformed.')
  }
  let lastAgainAt: number | null = null
  if (raw.lastAgainAt !== undefined && raw.lastAgainAt !== null) {
    if (!isValidTimestamp(raw.lastAgainAt)) {
      throw new Error('Backup file is malformed.')
    }
    lastAgainAt = raw.lastAgainAt
  }
  const rationale = typeof raw.rationale === 'string' && raw.rationale.trim() ? raw.rationale.trim() : undefined
  const sources = parseSources(raw.sources)
  const proposedBy = typeof raw.proposedBy === 'string' && raw.proposedBy.trim() ? raw.proposedBy.trim() : undefined
  const siblingGroupId =
    typeof raw.siblingGroupId === 'string' && raw.siblingGroupId.trim()
      ? raw.siblingGroupId.trim()
      : undefined
  const clozeIndex =
    typeof raw.clozeIndex === 'number' && Number.isInteger(raw.clozeIndex) && raw.clozeIndex >= 1 && raw.clozeIndex <= MAX_U32
      ? raw.clozeIndex
      : undefined
  return {
    front: raw.front,
    back: raw.back,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    tags: Array.isArray(raw.tags) && raw.tags.every((tag) => typeof tag === 'string') ? raw.tags : [],
    suspended: raw.suspended === true,
    lastAgainAt,
    scheduling: normalizeScheduling(raw.scheduling, now),
    reviews: parseReviews(raw.reviews),
    ...(rationale ? { rationale } : {}),
    ...(sources.length > 0 ? { sources } : {}),
    ...(proposedBy ? { proposedBy } : {}),
    ...(siblingGroupId ? { siblingGroupId } : {}),
    ...(clozeIndex ? { clozeIndex } : {}),
  }
}

function parseSources(raw: unknown): DraftSource[] {
  if (!Array.isArray(raw)) return []
  const out: DraftSource[] = []
  for (const item of raw) {
    if (!isObject(item) || typeof item.locator !== 'string') continue
    const locator = item.locator.trim()
    if (!locator) continue
    const label = typeof item.label === 'string' && item.label.trim() ? item.label.trim() : null
    out.push({ locator, label })
  }
  return out
}

function parseReviews(raw: unknown): ReviewBackup[] {
  if (raw === undefined) return []
  if (!Array.isArray(raw)) throw new Error('Backup file is malformed.')
  return raw.map((review) => {
    if (
      !isObject(review) ||
      !isValidTimestamp(review.reviewedAt) ||
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
  if (!isObject(v) || !isValidTimestamp(v.due)) return false
  if (v.kind === 'fsrs') {
    const validStep = v.step === undefined || v.step === null || isNonNegativeInt(v.step)
    const validStabilityFast =
      v.stabilityFast === undefined || v.stabilityFast === null || isFiniteNonNegative(v.stabilityFast)
    const validLastReview = v.lastReview === null || isValidTimestamp(v.lastReview)
    if (
      !isFiniteNonNegative(v.stability) ||
      !validStabilityFast ||
      !isFiniteNonNegative(v.difficulty) ||
      !isNonNegativeInt(v.reps) ||
      !isNonNegativeInt(v.lapses) ||
      !isNonNegativeInt(v.state, 3) ||
      !validStep ||
      !validLastReview
    ) {
      return false
    }
    if (v.reps > 0 && !(v.stability > 0)) return false
    return true
  }
  // legacy SM-2 (kind 'sm2' or absent)
  return (
    typeof v.repetitions === 'number' && Number.isFinite(v.repetitions) &&
    typeof v.intervalDays === 'number' && Number.isFinite(v.intervalDays) &&
    typeof v.easeFactor === 'number' && Number.isFinite(v.easeFactor)
  )
}

/** FSRS state passes through cleanly; any legacy SM-2 state is reset to a fresh FSRS
 *  card (due `now`), since SM-2 is no longer supported. */
function normalizeScheduling(v: Record<string, unknown>, now: number): SchedulingState {
  if (v.kind === 'fsrs') {
    const state: FSRSState = {
      kind: 'fsrs',
      stability: v.stability as number,
      difficulty: v.difficulty as number,
      reps: v.reps as number,
      lapses: v.lapses as number,
      state: v.state as number,
      step: typeof v.step === 'number' ? v.step : 0,
      lastReview: (v.lastReview as number | null) ?? null,
      due: v.due as number,
    }
    if (typeof v.stabilityFast === 'number') {
      state.stabilityFast = v.stabilityFast
    }
    return state
  }
  return initialFsrsState(now)
}
