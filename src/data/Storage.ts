import type { Card, Deck, ID, SchedulerKind, SchedulingState } from '../domain/models'
import type { DeckBackup } from './backup'

/** Outcome of an import: deck names added fresh vs. names that replaced existing decks. */
export interface ImportResult {
  added: string[]
  replaced: string[]
}

/** Fields of a card that can be patched after creation. */
export interface CardPatch {
  front?: string
  back?: string
  scheduling?: SchedulingState
}

/**
 * Persistence port for decks and cards.
 *
 * This is the seam between the app and where data lives. The MVP backs it with
 * IndexedDB (see {@link ./dexie/DexieStorage}); a future sync backend can
 * implement the same interface without the UI noticing.
 */
export interface Storage {
  createDeck(name: string, kind?: SchedulerKind): Promise<Deck>
  listDecks(): Promise<Deck[]>
  getDeck(id: ID): Promise<Deck | undefined>
  deleteDeck(id: ID): Promise<void>

  createCard(deckId: ID, front: string, back: string): Promise<Card>
  getCard(id: ID): Promise<Card | undefined>
  listCards(deckId: ID): Promise<Card[]>
  updateCard(id: ID, patch: CardPatch): Promise<void>
  deleteCard(id: ID): Promise<void>

  /** Cards in a deck due at or before `now`, soonest-due first. */
  dueCards(deckId: ID, now: number): Promise<Card[]>
  /** How many cards in a deck are due at or before `now`. */
  countDue(deckId: ID, now: number): Promise<number>

  /** Insert decks+cards; any existing deck whose name matches an incoming deck
   *  is removed first (replace-by-name). IDs are regenerated. */
  importDecks(decks: DeckBackup[]): Promise<ImportResult>
}
