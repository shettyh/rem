import type { ID, SchedulerKind, SchedulingState } from '../domain/models'
import { getScheduler } from '../domain/scheduler'
import type { Storage } from './Storage'

export interface CardBackup {
  front: string
  back: string
  createdAt: number
  updatedAt: number
  scheduling: SchedulingState
}

export interface DeckBackup {
  name: string
  createdAt: number
  schedulerKind: SchedulerKind
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
    const cards = await storage.listCards(id)
    out.push({
      name: deck.name,
      createdAt: deck.createdAt,
      schedulerKind: deck.schedulerKind,
      cards: cards.map((c) => ({
        front: c.front,
        back: c.back,
        createdAt: c.createdAt,
        updatedAt: c.updatedAt,
        scheduling: c.scheduling,
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
    scheduling: normalizeScheduling(raw.scheduling, now),
  }
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
  if (v.kind === 'fsrs') return v as unknown as SchedulingState
  return getScheduler('fsrs').initial(now)
}
