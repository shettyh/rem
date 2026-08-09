import type {
  Asset,
  Card,
  CardDraft,
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
} from '../domain/models'
import type { DeckBackup } from './backup'
import type { RepoSnapshot } from './sync/snapshot'
import type { DbOps } from './sync/merge'

/** Outcome of an import: deck names added fresh vs. names that replaced existing decks. */
export interface ImportResult {
  added: string[]
  replaced: string[]
}

export interface VersionedRepoSnapshot {
  snapshot: RepoSnapshot
  revision: number
}

export type ApplyMergeResult =
  | { status: 'applied'; revision: number }
  | { status: 'stale'; currentRevision: number }

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

/** One atomic persisted review outcome. `fsrsGrade` is omitted for fixed steps. */
export interface ReviewCommit {
  cardId: ID
  deckId: ID
  patch: CardPatch
  reviewedAt: number
  fsrsGrade?: Grade
  daily?: { day: string; field: 'newIntroduced' | 'reviewsDone' }
}

/**
 * Native application port for collection persistence and study sessions.
 *
 * Packaged builds use SQLite and the shared Rust StudySession through
 * {@link ./TauriStorage}; browser tests inject the Dexie adapter without
 * changing feature code.
 */
export interface Storage {
  /** Re-run storage-backed UI queries after a mutation through this adapter. */
  subscribe(listener: () => void): () => void

  createDeck(name: string, kind?: SchedulerKind): Promise<Deck>
  listDecks(): Promise<Deck[]>
  getDeck(id: ID): Promise<Deck | undefined>
  deleteDeck(id: ID): Promise<void>
  updateDeck(id: ID, patch: DeckPatch): Promise<void>

  createCard(deckId: ID, front: string, back: string, tags?: string[]): Promise<Card>
  getCard(id: ID): Promise<Card | undefined>
  listCards(deckId: ID): Promise<Card[]>
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
  endStudy(sessionId: string): Promise<void>

  /** Atomically persist card/counter changes and an optional FSRS training event. */
  commitReview(commit: ReviewCommit): Promise<ReviewLog | null>
  listReviewLogs(deckId: ID): Promise<ReviewLog[]>

  /** Cards in a deck due at or before `now`, soonest-due first. */
  dueCards(deckId: ID, now: number): Promise<Card[]>
  /** How many cards in a deck are due at or before `now`. */
  countDue(deckId: ID, now: number): Promise<number>

  /** Today's cap counters for a deck; zeros when the day has no row yet. */
  getDailyStat(deckId: ID, day: string): Promise<{ newIntroduced: number; reviewsDone: number }>

  /** Insert decks+cards; any existing deck whose name matches an incoming deck
   *  is removed first (replace-by-name). IDs are regenerated. */
  importDecks(decks: DeckBackup[]): Promise<ImportResult>

  /** Full point-in-time snapshot and the local revision observed with it. */
  exportSnapshot(): Promise<VersionedRepoSnapshot>
  /** Apply a merge only if the local store is still at `expectedRevision`. */
  applyMerge(ops: DbOps, expectedRevision: number): Promise<ApplyMergeResult>

  // Assets (images/GIFs embedded in card markdown as asset:<hash>)
  putAsset(bytes: Uint8Array, mime: string): Promise<Asset>
  getAsset(hash: ID): Promise<Asset | undefined>
  sweepOrphanAssets(): Promise<void>
}
