import type { ID, SchedulingState } from '../domain/models'
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

export function parseBackup(text: string): DeckBackup[] {
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
  return data.decks.map(parseDeck)
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

function parseDeck(raw: unknown): DeckBackup {
  if (
    !isObject(raw) ||
    typeof raw.name !== 'string' ||
    typeof raw.createdAt !== 'number' ||
    !Array.isArray(raw.cards)
  ) {
    throw new Error('Backup file is malformed.')
  }
  return { name: raw.name, createdAt: raw.createdAt, cards: raw.cards.map(parseCard) }
}

function parseCard(raw: unknown): CardBackup {
  if (
    !isObject(raw) ||
    typeof raw.front !== 'string' ||
    typeof raw.back !== 'string' ||
    typeof raw.createdAt !== 'number' ||
    typeof raw.updatedAt !== 'number' ||
    !isScheduling(raw.scheduling)
  ) {
    throw new Error('Backup file is malformed.')
  }
  return {
    front: raw.front,
    back: raw.back,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    scheduling: raw.scheduling,
  }
}

function isScheduling(v: unknown): v is SchedulingState {
  return (
    isObject(v) &&
    typeof v.repetitions === 'number' &&
    typeof v.intervalDays === 'number' &&
    typeof v.easeFactor === 'number' &&
    typeof v.due === 'number'
  )
}
