import { invoke } from '@tauri-apps/api/core'
import type {
  ApkgImportPreview,
  ApkgImportResult,
  Asset,
  Card,
  CardDraft,
  CardSearchHit,
  CollectionStats,
  Deck,
  DraftDecision,
  DraftResolution,
  Grade,
  ID,
  NewDraftInput,
  ProposalMetadata,
  ProposeDraftsResult,
  ReviewLog,
  SchedulerKind,
  StartedStudy,
  StudyGradeOutcome,
  StudyRequest,
  StudyView,
} from '../domain/models'
import type { DeckBackup } from './backup'
import type {
  CardPatch,
  DeckPatch,
  ImportResult,
  ReviewCommit,
  Storage,
} from './Storage'

export type StorageInvoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>

type NativeAsset = Omit<Asset, 'bytes'> & { bytes: number[] }

/** Native production adapter. All persistence rules remain in rem-core. */
export class TauriStorage implements Storage {
  private readonly listeners = new Set<() => void>()

  constructor(
    private readonly call: StorageInvoke = invoke,
    private readonly now: () => number = Date.now,
  ) {}

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  notifyExternalChange(): void {
    this.notify()
  }

  async createDeck(name: string, _kind: SchedulerKind = 'fsrs'): Promise<Deck> {
    const deck = await this.call<Deck>('storage_create_deck', { name, now: this.now() })
    this.notify()
    return deck
  }

  listDecks(): Promise<Deck[]> {
    return this.call('storage_list_decks')
  }

  async getDeck(id: ID): Promise<Deck | undefined> {
    return (await this.call<Deck | null>('storage_get_deck', { id })) ?? undefined
  }

  async deleteDeck(id: ID): Promise<void> {
    await this.call('storage_delete_deck', { id, now: this.now() })
    this.notify()
  }

  async updateDeck(id: ID, patch: DeckPatch): Promise<void> {
    await this.call('storage_update_deck', { id, patch, now: this.now() })
    this.notify()
  }

  async createCard(
    deckId: ID,
    front: string,
    back: string,
    tags: string[] = [],
  ): Promise<Card> {
    const card = await this.call<Card>('storage_create_card', {
      deckId,
      front,
      back,
      tags,
      now: this.now(),
    })
    this.notify()
    return card
  }

  async createCardOrCloze(
    deckId: ID,
    front: string,
    back: string,
    tags: string[] = [],
  ): Promise<Card[]> {
    const cards = await this.call<Card[]>('storage_create_card_or_cloze', {
      deckId,
      front,
      back,
      tags,
      now: this.now(),
    })
    this.notify()
    return cards
  }

  async updateClozeNote(
    cardId: ID,
    front: string,
    back: string,
    tags: string[] = [],
  ): Promise<Card[]> {
    const cards = await this.call<Card[]>('storage_update_cloze_note', {
      cardId,
      front,
      back,
      tags,
      now: this.now(),
    })
    this.notify()
    return cards
  }

  async getCard(id: ID): Promise<Card | undefined> {
    return (await this.call<Card | null>('storage_get_card', { id })) ?? undefined
  }

  listCards(deckId: ID): Promise<Card[]> {
    return this.call('storage_list_cards', { deckId })
  }

  searchCards(
    query: string,
    options?: { deckId?: ID; limit?: number },
  ): Promise<CardSearchHit[]> {
    return this.call('storage_search_cards', {
      query,
      deckId: options?.deckId ?? null,
      limit: options?.limit ?? 50,
    })
  }

  async updateCard(id: ID, patch: CardPatch): Promise<void> {
    await this.call('storage_update_card', { id, patch, now: this.now() })
    this.notify()
  }

  async deleteCard(id: ID): Promise<void> {
    await this.call('storage_delete_card', { id, now: this.now() })
    this.notify()
  }

  async proposeDrafts(
    deckId: ID,
    inputs: NewDraftInput[],
    metadata: ProposalMetadata,
    dryRun = false,
  ): Promise<ProposeDraftsResult> {
    const result = await this.call<ProposeDraftsResult>('storage_propose_drafts', {
      deckId,
      inputs,
      metadata,
      now: this.now(),
      dryRun,
    })
    if (!dryRun && result.outcomes.some((outcome) => outcome.status === 'created')) this.notify()
    return result
  }

  listDrafts(): Promise<CardDraft[]> {
    return this.call('storage_list_drafts')
  }

  async resolveDraft(
    draftId: ID,
    expectedRevision: number,
    decision: DraftDecision,
  ): Promise<DraftResolution> {
    const result = await this.call<DraftResolution>('storage_resolve_draft', {
      draftId,
      expectedRevision,
      decision,
      now: this.now(),
    })
    this.notify()
    return result
  }

  startStudy(request: StudyRequest): Promise<StartedStudy> {
    return this.call('study_start', { request, now: this.now() })
  }

  revealStudy(sessionId: string): Promise<StudyView> {
    return this.call('study_reveal', { sessionId, now: this.now() })
  }

  async gradeStudy(sessionId: string, grade: Grade): Promise<StudyGradeOutcome> {
    const outcome = await this.call<StudyGradeOutcome>('study_grade', {
      sessionId,
      grade,
      now: this.now(),
    })
    if (outcome.status === 'graded') this.notify()
    return outcome
  }

  advanceStudyPreview(sessionId: string): Promise<StudyView> {
    return this.call('study_advance_preview', { sessionId, now: this.now() })
  }

  async undoStudyGrade(sessionId: string): Promise<StudyView> {
    const view = await this.call<StudyView>('study_undo_last_grade', {
      sessionId,
      now: this.now(),
    })
    this.notify()
    return view
  }

  async suspendStudyCard(sessionId: string): Promise<StudyView> {
    const view = await this.call<StudyView>('study_suspend_current', {
      sessionId,
      now: this.now(),
    })
    this.notify()
    return view
  }

  endStudy(sessionId: string): Promise<void> {
    return this.call('study_end', { sessionId })
  }

  async commitReview(commit: ReviewCommit): Promise<ReviewLog | null> {
    const log = await this.call<ReviewLog | null>('storage_commit_review', { commit })
    this.notify()
    return log
  }

  listReviewLogs(deckId: ID): Promise<ReviewLog[]> {
    return this.call('storage_list_review_logs', { deckId })
  }

  getStats(deckId: ID | null, days: number, now?: number): Promise<CollectionStats> {
    return this.call('storage_get_stats', {
      deckId,
      days,
      now: now ?? this.now(),
    })
  }

  dueCards(deckId: ID, now: number): Promise<Card[]> {
    return this.call('storage_due_cards', { deckId, now })
  }

  countDue(deckId: ID, now: number): Promise<number> {
    return this.call('storage_count_due', { deckId, now })
  }

  getDailyStat(
    deckId: ID,
    day: string,
  ): Promise<{ newIntroduced: number; reviewsDone: number }> {
    return this.call('storage_get_daily_stat', { deckId, day })
  }

  async importDecks(decks: DeckBackup[]): Promise<ImportResult> {
    const result = await this.call<ImportResult>('storage_import_decks', {
      decks,
      now: this.now(),
    })
    this.notify()
    return result
  }

  previewApkg(bytes: Uint8Array): Promise<ApkgImportPreview> {
    return this.call<ApkgImportPreview>('storage_preview_apkg', {
      bytes: Array.from(bytes),
      now: this.now(),
    })
  }

  async importApkg(bytes: Uint8Array): Promise<ApkgImportResult> {
    const result = await this.call<ApkgImportResult>('storage_import_apkg', {
      bytes: Array.from(bytes),
      now: this.now(),
    })
    this.notify()
    return result
  }

  async putAsset(bytes: Uint8Array, mime: string): Promise<Asset> {
    const asset = await this.call<NativeAsset>('storage_put_asset', {
      bytes: Array.from(bytes),
      mime,
      now: this.now(),
    })
    this.notify()
    return fromNativeAsset(asset)
  }

  async getAsset(hash: ID): Promise<Asset | undefined> {
    const asset = await this.call<NativeAsset | null>('storage_get_asset', { hash })
    return asset ? fromNativeAsset(asset) : undefined
  }

  async sweepOrphanAssets(): Promise<void> {
    await this.call<number>('storage_sweep_orphan_assets')
    this.notify()
  }

  private notify(): void {
    for (const listener of this.listeners) listener()
  }
}

function fromNativeAsset(asset: NativeAsset): Asset {
  return { ...asset, bytes: new Uint8Array(asset.bytes) }
}
