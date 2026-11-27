import type {
  Asset,
  Card,
  CardDraft,
  CardSearchHit,
  CollectionStats,
  Deck,
  DraftDecision,
  DraftProposalOutcome,
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
} from '../../domain/models'
import { DEFAULT_DECK_SETTINGS, FSRS_STATE } from '../../domain/models'
import { deckColor } from '../../ui/deckColor'
import { hashBytes } from '../assetHash'
import { assetRefs } from '../assetRefs'
import { getScheduler } from '../../domain/scheduler'
import { BrowserStudySession, type ReviewRollback } from '../../features/review/browserStudy'
import { buildStats, type StatsRange } from '../../features/stats/stats'
import { localDay } from '../../features/review/day'
import type {
  CardPatch,
  DeckPatch,
  ImportResult,
  ReviewCommit,
  Storage,
} from '../Storage'
import { planImport, type DeckBackup } from '../backup'
import type { RemDB } from './db'

/** IndexedDB-backed {@link Storage}, using Dexie. */
export class DexieStorage implements Storage {
  private readonly listeners = new Set<() => void>()
  private readonly studySessions = new Map<string, BrowserStudySession>()

  constructor(readonly db: RemDB) {}

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  async createDeck(name: string, kind: SchedulerKind = 'fsrs'): Promise<Deck> {
    const now = Date.now()
    const id = crypto.randomUUID()
    const deck: Deck = {
      id,
      name: name.trim(),
      createdAt: now,
      updatedAt: now,
      color: deckColor(id),
      schedulerKind: kind,
      settings: { ...DEFAULT_DECK_SETTINGS },
    }
    await this.db.decks.add(deck)
    this.notify()
    return deck
  }

  async updateDeck(id: ID, patch: DeckPatch): Promise<void> {
    await this.db.decks.update(id, { ...patch, updatedAt: Date.now() })
    this.notify()
  }

  listDecks(): Promise<Deck[]> {
    return this.db.decks.orderBy('createdAt').toArray()
  }

  getDeck(id: ID): Promise<Deck | undefined> {
    return this.db.decks.get(id)
  }

  async deleteDeck(id: ID): Promise<void> {
    await this.db.transaction(
      'rw',
      [this.db.decks, this.db.cards, this.db.cardDrafts, this.db.tombstones, this.db.dailyStats,
        this.db.reviewLogs],
      async () => {
        await this.db.cards.where('deckId').equals(id).delete()
        await this.db.cardDrafts.where('deckId').equals(id).delete()
        await this.db.dailyStats.where('deckId').equals(id).delete()
        await this.db.reviewLogs.where('deckId').equals(id).delete()
        await this.db.decks.delete(id)
        await this.db.tombstones.put({ id, kind: 'deck', deletedAt: Date.now() })
      },
    )
    this.notify()
  }

  async createCard(deckId: ID, front: string, back: string, tags: string[] = []): Promise<Card> {
    const now = Date.now()
    const card: Card = {
      id: crypto.randomUUID(),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      tags,
      suspended: false,
      lastAgainAt: null,
      scheduling: getScheduler().initial(now),
    }
    await this.db.cards.add(card)
    this.notify()
    return card
  }

  getCard(id: ID): Promise<Card | undefined> {
    return this.db.cards.get(id)
  }

  listCards(deckId: ID): Promise<Card[]> {
    return this.db.cards.where('deckId').equals(deckId).sortBy('createdAt')
  }

  async searchCards(
    query: string,
    options?: { deckId?: ID; limit?: number },
  ): Promise<CardSearchHit[]> {
    const trimmed = query.trim().toLocaleLowerCase()
    if (!trimmed) return []
    const limit = Math.min(options?.limit ?? 50, 200)
    const [decks, cards] = await Promise.all([
      this.db.decks.toArray(),
      options?.deckId
        ? this.db.cards.where('deckId').equals(options.deckId).toArray()
        : this.db.cards.toArray(),
    ])
    const deckById = new Map(decks.map((d) => [d.id, d]))
    const tokens = trimmed.split(/\s+/).filter(Boolean)
    const hits: CardSearchHit[] = []
    for (const card of cards) {
      const deck = deckById.get(card.deckId)
      if (!deck) continue
      const haystack = [
        card.front,
        card.back,
        card.tags.join(' '),
        card.rationale ?? '',
      ].join('\n').toLocaleLowerCase()
      if (tokens.every((token) => haystack.includes(token))) {
        hits.push({ card, deckName: deck.name, deckColor: deck.color })
      }
    }
    hits.sort((a, b) => b.card.updatedAt - a.card.updatedAt)
    return hits.slice(0, limit)
  }

  async updateCard(id: ID, patch: CardPatch): Promise<void> {
    await this.db.cards.update(id, { ...patch, updatedAt: Date.now() })
    this.notify()
  }

  async proposeDrafts(
    deckId: ID,
    inputs: NewDraftInput[],
    metadata: ProposalMetadata,
    dryRun = false,
  ): Promise<ProposeDraftsResult> {
    if (inputs.length === 0) throw new Error('invalid input: draft batch must not be empty')
    const now = Date.now()
    const proposedBy = normalizeOptional(metadata.proposedBy)
    const prepared = inputs.map((input): CardDraft => {
      if (!input.front.trim()) throw new Error('invalid input: draft front must not be blank')
      const sources = input.sources.map((source) => {
        const locator = source.locator.trim()
        if (!locator) throw new Error('invalid input: draft source locator must not be blank')
        return { locator, label: normalizeOptional(source.label) }
      })
      return {
        id: crypto.randomUUID(),
        deckId,
        front: input.front,
        back: input.back,
        tags: normalizeUserTags(input.tags),
        rationale: normalizeOptional(input.rationale),
        sources,
        proposedBy,
        createdAt: now,
        updatedAt: now,
        revision: 0,
      }
    })

    const result = await this.db.transaction(
      'rw',
      this.db.decks,
      this.db.cards,
      this.db.cardDrafts,
      async (): Promise<ProposeDraftsResult> => {
        if (!await this.db.decks.get(deckId)) throw new Error(`deck not found: ${deckId}`)
        const cards = await this.db.cards.where('deckId').equals(deckId).toArray()
        const drafts = await this.db.cardDrafts.where('deckId').equals(deckId).toArray()
        const planned: CardDraft[] = []
        const outcomes: DraftProposalOutcome[] = []
        for (const draft of prepared) {
          const card = cards.find((candidate) => sameContent(candidate, draft))
          if (card) {
            outcomes.push({ status: 'duplicateCard', value: card })
            continue
          }
          const duplicate = drafts.find((candidate) => sameContent(candidate, draft))
            ?? planned.find((candidate) => sameContent(candidate, draft))
          if (duplicate) {
            outcomes.push({ status: 'duplicateDraft', value: duplicate })
            continue
          }
          if (!dryRun) await this.db.cardDrafts.add(draft)
          planned.push(draft)
          outcomes.push({ status: 'created', value: draft })
        }
        return { outcomes }
      },
    )
    if (!dryRun && result.outcomes.some((outcome) => outcome.status === 'created')) {
      this.notify()
    }
    return result
  }

  listDrafts(): Promise<CardDraft[]> {
    return this.db.cardDrafts.orderBy('createdAt').toArray()
  }

  async resolveDraft(
    id: ID,
    expectedRevision: number,
    decision: DraftDecision,
  ): Promise<DraftResolution> {
    const now = Date.now()
    const result = await this.db.transaction(
      'rw',
      this.db.decks,
      this.db.cards,
      this.db.cardDrafts,
      async (): Promise<DraftResolution> => {
        const draft = await this.db.cardDrafts.get(id)
        if (!draft) throw new Error(`draft not found: ${id}`)
        if (draft.revision !== expectedRevision) throw new Error(`draft changed: ${id}`)
        if (decision.decision === 'reject') {
          await this.db.cardDrafts.delete(id)
          return { status: 'rejected' }
        }
        if (!await this.db.decks.get(decision.deckId)) {
          throw new Error(`deck not found: ${decision.deckId}`)
        }
        if (!decision.card.front.trim()) throw new Error('invalid input: card front must not be blank')
        const tags = normalizeUserTags(decision.card.tags)
        const cards = await this.db.cards.where('deckId').equals(decision.deckId).toArray()
        const existing = cards.find((card) =>
          card.front === decision.card.front
          && card.back === decision.card.back
          && sameTags(card.tags, tags))
        await this.db.cardDrafts.delete(id)
        if (existing) return { status: 'existingCard', value: existing }
        const card: Card = {
          id: crypto.randomUUID(),
          deckId: decision.deckId,
          front: decision.card.front,
          back: decision.card.back,
          createdAt: now,
          updatedAt: now,
          tags,
          suspended: false,
          lastAgainAt: null,
          scheduling: getScheduler().initial(now),
          rationale: draft.rationale,
          sources: draft.sources,
          proposedBy: draft.proposedBy,
        }
        await this.db.cards.add(card)
        return { status: 'accepted', value: card }
      },
    )
    this.notify()
    return result
  }

  async startStudy(request: StudyRequest): Promise<StartedStudy> {
    const session = await BrowserStudySession.start(
      this,
      request,
      Date.now(),
      (rollback) => this.rollbackReview(rollback),
    )
    const sessionId = `study-${crypto.randomUUID()}`
    this.studySessions.set(sessionId, session)
    return { sessionId, view: session.view() }
  }

  revealStudy(sessionId: string): Promise<StudyView> {
    return this.studySession(sessionId).reveal(Date.now())
  }

  gradeStudy(sessionId: string, grade: Grade): Promise<StudyGradeOutcome> {
    return this.studySession(sessionId).grade(grade, Date.now())
  }

  async advanceStudyPreview(sessionId: string): Promise<StudyView> {
    return this.studySession(sessionId).advancePreview(Date.now())
  }

  undoStudyGrade(sessionId: string): Promise<StudyView> {
    return this.studySession(sessionId).undoLastGrade()
  }

  suspendStudyCard(sessionId: string): Promise<StudyView> {
    return this.studySession(sessionId).suspendCurrent(Date.now())
  }

  async endStudy(sessionId: string): Promise<void> {
    this.studySessions.delete(sessionId)
  }

  private async rollbackReview(rollback: ReviewRollback): Promise<void> {
    await this.db.transaction('rw', this.db.cards, this.db.dailyStats, this.db.reviewLogs, async () => {
      await this.db.cards.update(rollback.cardId, { ...rollback.patch, updatedAt: Date.now() })
      if (rollback.reviewLogId) {
        await this.db.reviewLogs.delete(rollback.reviewLogId)
      }
      if (rollback.daily) {
        const id = `${rollback.deckId}:${rollback.daily.day}`
        const row = await this.db.dailyStats.get(id)
        if (row) {
          const field = rollback.daily.field
          await this.db.dailyStats.put({ ...row, [field]: Math.max(0, row[field] - 1) })
        }
      }
    })
    this.notify()
  }

  async commitReview(commit: ReviewCommit): Promise<ReviewLog | null> {
    const result = await this.db.transaction('rw', this.db.cards, this.db.dailyStats, this.db.reviewLogs, async () => {
      await this.db.cards.update(commit.cardId, { ...commit.patch, updatedAt: commit.reviewedAt })
      if (commit.daily) {
        const id = `${commit.deckId}:${commit.daily.day}`
        const row = await this.db.dailyStats.get(id)
        const base = row ?? {
          id,
          deckId: commit.deckId,
          day: commit.daily.day,
          newIntroduced: 0,
          reviewsDone: 0,
        }
        const field = commit.daily.field
        await this.db.dailyStats.put({ ...base, [field]: base[field] + 1 })
      }
      if (!commit.fsrsGrade) return null
      const log: ReviewLog = {
        id: crypto.randomUUID(),
        deckId: commit.deckId,
        cardId: commit.cardId,
        reviewedAt: commit.reviewedAt,
        grade: commit.fsrsGrade,
      }
      await this.db.reviewLogs.add(log)
      return log
    })
    this.notify()
    return result
  }

  listReviewLogs(deckId: ID): Promise<ReviewLog[]> {
    return this.db.reviewLogs.where('deckId').equals(deckId).sortBy('reviewedAt')
  }

  async getStats(deckId: ID | null, days: number, now = Date.now()): Promise<CollectionStats> {
    const [decks, logs, cards] = await Promise.all([
      this.listDecks(),
      this.db.reviewLogs.toArray(),
      deckId
        ? this.db.cards.where('deckId').equals(deckId).toArray()
        : this.db.cards.toArray(),
    ])
    const range = (days === 7 || days === 30 || days === 90 ? days : 30) as StatsRange
    const summary = buildStats(logs, decks, now, range, deckId)

    const todayStart = new Date(now)
    todayStart.setHours(0, 0, 0, 0)
    const upcoming7d = Array.from({ length: 7 }, (_, index) => {
      const date = new Date(todayStart.getFullYear(), todayStart.getMonth(), todayStart.getDate() + index)
      return { day: localDay(date.getTime()), timestampMs: date.getTime(), count: 0 }
    })
    const dayIndexByKey = new Map(upcoming7d.map((entry, index) => [entry.day, index]))
    for (const card of cards) {
      if (card.suspended || card.scheduling.state === FSRS_STATE.New) continue
      const due = card.scheduling.due
      if (due <= now) {
        upcoming7d[0].count += 1
        continue
      }
      const idx = dayIndexByKey.get(localDay(due))
      if (idx !== undefined) upcoming7d[idx].count += 1
    }

    return {
      historyCount: summary.historyCount,
      totalReviews: summary.totalReviews,
      recallRate: summary.recallRate,
      currentStreakDays: summary.currentStreak,
      activeDays: summary.activeDays,
      grades: summary.grades,
      daily: summary.daily.map((d) => ({
        day: d.day,
        timestampMs: d.timestamp,
        count: d.count,
      })),
      byDeck: summary.byDeck.map((item) => ({
        deckId: item.deck.id,
        deckName: item.deck.name,
        deckColor: item.deck.color,
        reviews: item.reviews,
        recallRate: item.recallRate,
      })),
      upcoming7d,
    }
  }

  async deleteCard(id: ID): Promise<void> {
    await this.db.transaction('rw', this.db.cards, this.db.tombstones, this.db.reviewLogs, async () => {
      await this.db.reviewLogs.where('cardId').equals(id).delete()
      await this.db.cards.delete(id)
      await this.db.tombstones.put({ id, kind: 'card', deletedAt: Date.now() })
    })
    this.notify()
  }

  async dueCards(deckId: ID, now: number): Promise<Card[]> {
    const cards = await this.db.cards.where('deckId').equals(deckId).toArray()
    return cards
      .filter((c) => !c.suspended && c.scheduling.due <= now)
      .sort((a, b) => a.scheduling.due - b.scheduling.due)
  }

  async countDue(deckId: ID, now: number): Promise<number> {
    const cards = await this.dueCards(deckId, now)
    return cards.length
  }

  async getDailyStat(deckId: ID, day: string): Promise<{ newIntroduced: number; reviewsDone: number }> {
    const row = await this.db.dailyStats.get(`${deckId}:${day}`)
    return { newIntroduced: row?.newIntroduced ?? 0, reviewsDone: row?.reviewsDone ?? 0 }
  }

  async importDecks(decks: DeckBackup[]): Promise<ImportResult> {
    const incomingNames = decks.map((d) => d.name)
    const result = await this.db.transaction('rw', this.db.decks, this.db.cards, this.db.cardDrafts, this.db.reviewLogs, async () => {
      const existing = await this.db.decks.toArray()
      const result = planImport(incomingNames, existing.map((d) => d.name))

      const toReplace = new Set(result.replaced)
      const deckIdsToDelete = existing.filter((d) => toReplace.has(d.name)).map((d) => d.id)
      for (const id of deckIdsToDelete) {
        await this.db.cards.where('deckId').equals(id).delete()
        await this.db.cardDrafts.where('deckId').equals(id).delete()
        await this.db.reviewLogs.where('deckId').equals(id).delete()
        await this.db.decks.delete(id)
      }

      for (const d of decks) {
        const deckId = crypto.randomUUID()
        await this.db.decks.add({
          id: deckId,
          name: d.name,
          createdAt: d.createdAt,
          updatedAt: Date.now(),
          color: d.color ?? deckColor(deckId),
          schedulerKind: d.schedulerKind,
          settings: d.settings,
        })
        for (const c of d.cards) {
          const cardId = crypto.randomUUID()
          await this.db.cards.add({
            id: cardId,
            deckId,
            front: c.front,
            back: c.back,
            createdAt: c.createdAt,
            updatedAt: c.updatedAt,
            tags: c.tags,
            suspended: c.suspended,
            lastAgainAt: c.lastAgainAt,
            scheduling: c.scheduling,
            rationale: c.rationale ?? null,
            sources: c.sources ?? [],
            proposedBy: c.proposedBy ?? null,
          })
          for (const review of c.reviews) {
            await this.db.reviewLogs.add({
              id: crypto.randomUUID(),
              deckId,
              cardId,
              reviewedAt: review.reviewedAt,
              grade: review.grade,
            })
          }
        }
      }
      return result
    })
    this.notify()
    return result
  }

  async putAsset(bytes: Uint8Array, mime: string): Promise<Asset> {
    const hash = await hashBytes(bytes)
    const asset = await this.db.transaction('rw', this.db.assets, async () => {
      const existing = await this.db.assets.get(hash)
      if (existing) return existing
      const created: Asset = { hash, mime, bytes, createdAt: Date.now() }
      await this.db.assets.add(created)
      return created
    })
    this.notify()
    return asset
  }

  getAsset(hash: ID): Promise<Asset | undefined> {
    return this.db.assets.get(hash)
  }

  async sweepOrphanAssets(): Promise<void> {
    const [cards, assets] = await Promise.all([this.db.cards.toArray(), this.db.assets.toArray()])
    const referenced = new Set(cards.flatMap((c) => [...assetRefs(c.front), ...assetRefs(c.back)]))
    const orphans = assets.filter((a) => !referenced.has(a.hash)).map((a) => a.hash)
    if (orphans.length) await this.db.assets.bulkDelete(orphans)
    this.notify()
  }

  private studySession(sessionId: string): BrowserStudySession {
    const session = this.studySessions.get(sessionId)
    if (!session) throw new Error(`study session not found: ${sessionId}`)
    return session
  }

  private notify(): void {
    for (const listener of this.listeners) listener()
  }
}

function normalizeOptional(value: string | null): string | null {
  const normalized = value?.trim() ?? ''
  return normalized || null
}

function normalizeUserTags(tags: string[]): string[] {
  const seen = new Set<string>()
  const normalized: string[] = []
  for (const value of tags) {
    const tag = value.trim()
    const key = tag.toLocaleLowerCase()
    if (!tag || key === 'leech' || seen.has(key)) continue
    seen.add(key)
    normalized.push(tag)
  }
  return normalized
}

function sameTags(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((tag, index) => tag === right[index])
}

function sameContent(
  left: Pick<Card, 'front' | 'back' | 'tags'> | Pick<CardDraft, 'front' | 'back' | 'tags'>,
  right: Pick<CardDraft, 'front' | 'back' | 'tags'>,
): boolean {
  return left.front === right.front && left.back === right.back && sameTags(left.tags, right.tags)
}
