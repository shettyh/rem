import type {
  ApkgImportPreview,
  ApkgImportResult,
  Asset,
  Card,
  CardDraft,
  CardSearchHit,
  CollectionStats,
  Deck,
  DeckSettings,
  DraftDecision,
  DraftResolution,
  Grade,
  ID,
  NewDraftInput,
  ProposalMetadata,
  ProposeDraftsResult,
  ReviewLog,
  SchedulerKind,
  SchedulingState,
  StartedStudy,
  StudyGradeOutcome,
  StudyRequest,
  StudyView,
  TodaySummary,
} from '../domain/models'
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
  tags?: string[]
  suspended?: boolean
  lastAgainAt?: number | null
  scheduling?: SchedulingState
}

export interface DeckPatch {
  name?: string
  color?: string
  settings?: DeckSettings
}

/**
 * Native application port for collection persistence and study sessions.
 *
 * Packaged builds use SQLite and the shared Rust StudySession through
 * {@link ./TauriStorage}; frontend tests inject an in-memory stub without
 * changing feature code.
 */
export interface Storage {
  /** Re-run storage-backed UI queries after a mutation through this adapter. */
  subscribe(listener: () => void): () => void
  /** Notify subscribers after external mutations (e.g., cloud sync pulling remote ops). */
  notifyExternalChange?(): void

  createDeck(name: string, kind?: SchedulerKind): Promise<Deck>
  listDecks(): Promise<Deck[]>
  getDeck(id: ID): Promise<Deck | undefined>
  deleteDeck(id: ID): Promise<void>
  deleteDeckTree(path: string): Promise<ID[]>
  updateDeck(id: ID, patch: DeckPatch): Promise<void>

  createCard(deckId: ID, front: string, back: string, tags?: string[]): Promise<Card>
  createCardOrCloze(deckId: ID, front: string, back: string, tags?: string[]): Promise<Card[]>
  updateClozeNote(cardId: ID, front: string, back: string, tags?: string[]): Promise<Card[]>
  getCard(id: ID): Promise<Card | undefined>
  listCards(deckId: ID): Promise<Card[]>
  searchCards(
    query: string,
    options?: { deckId?: ID; limit?: number },
  ): Promise<CardSearchHit[]>
  updateCard(id: ID, patch: CardPatch): Promise<void>
  deleteCard(id: ID): Promise<void>

  proposeDrafts(
    deckId: ID,
    inputs: NewDraftInput[],
    metadata: ProposalMetadata,
    dryRun?: boolean,
  ): Promise<ProposeDraftsResult>
  listDrafts(): Promise<CardDraft[]>
  resolveDraft(
    id: ID,
    expectedRevision: number,
    decision: DraftDecision,
  ): Promise<DraftResolution>

  startStudy(request: StudyRequest): Promise<StartedStudy>
  revealStudy(sessionId: string): Promise<StudyView>
  gradeStudy(sessionId: string, grade: Grade): Promise<StudyGradeOutcome>
  advanceStudyPreview(sessionId: string): Promise<StudyView>
  undoStudyGrade(sessionId: string): Promise<StudyView>
  suspendStudyCard(sessionId: string): Promise<StudyView>
  endStudy(sessionId: string): Promise<void>

  listReviewLogs(deckId: ID): Promise<ReviewLog[]>
  getTodaySummary(now?: number): Promise<TodaySummary>
  getStats(deckId: ID | null, days: number, now?: number): Promise<CollectionStats>

  /** Insert decks+cards; any existing deck whose name matches an incoming deck
   *  is removed first (replace-by-name). IDs are regenerated. */
  importDecks(decks: DeckBackup[]): Promise<ImportResult>
  previewApkg?(bytes: Uint8Array): Promise<ApkgImportPreview>
  importApkg?(bytes: Uint8Array): Promise<ApkgImportResult>

  // Assets (images/GIFs embedded in card markdown as asset:<hash>)
  putAsset(bytes: Uint8Array, mime: string): Promise<Asset>
  getAsset(hash: ID): Promise<Asset | undefined>
  sweepOrphanAssets(): Promise<void>
}
