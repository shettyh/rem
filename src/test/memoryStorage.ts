import type {
  Asset,
  Card,
  CardDraft,
  CardSearchHit,
  CollectionStats,
  Deck,
  DeckTodaySummary,
  DraftDecision,
  DraftProposalOutcome,
  DraftResolution,
  FSRSState,
  Grade,
  ID,
  LeechAction,
  NewDraftInput,
  ProposalMetadata,
  ProposeDraftsResult,
  ReviewLog,
  SchedulerKind,
  StartedStudy,
  StudyGradeOutcome,
  StudyRequest,
  StudyView,
  TodaySummary,
} from '../domain/models'
import { DEFAULT_DECK_SETTINGS, FSRS_STATE, MS_PER_DAY, initialFsrsState } from '../domain/models'
import type { CardPatch, DeckPatch, ImportResult, Storage } from '../data/Storage'
import { planImport, type DeckBackup } from '../data/backup'
import { extractSiblingCardIndices } from '../features/cards/occlusion'
import {
  deckAncestorPaths,
  deckIsDescendantOrSelf,
  deckIsStrictDescendant,
  normalizeDeckName,
} from '../features/decks/deckTree'
import { deckColor } from '../ui/deckColor'

type StatsProvider = (deckId: ID | null, days: number, now: number) => CollectionStats

interface StubSession {
  queue: Card[]
  current: Card | null
  revealed: boolean
  nextStates: Record<Grade, FSRSState> | null
  reviewed: number
  preview: boolean
  showTimer: boolean
  notice: LeechAction | null
  lastUndo: { card: Card; previousNotice: LeechAction | null } | null
}

function emptyStats(): CollectionStats {
  return {
    historyCount: 0,
    totalReviews: 0,
    recallRate: null,
    currentStreakDays: 0,
    activeDays: 0,
    grades: { again: 0, hard: 0, good: 0, easy: 0 },
    daily: [],
    byDeck: [],
    upcoming7d: [],
  }
}

function cannedNextStates(card: Card, now: number): Record<Grade, FSRSState> {
  const make = (offsetMs: number): FSRSState => ({
    kind: 'fsrs',
    stability: Math.max(1, card.scheduling.stability),
    difficulty: card.scheduling.difficulty || 5,
    reps: card.scheduling.reps + 1,
    lapses: card.scheduling.lapses,
    state: FSRS_STATE.Review,
    step: 0,
    lastReview: now,
    due: now + offsetMs,
  })
  return {
    again: make(60_000),
    hard: make(MS_PER_DAY),
    good: make(3 * MS_PER_DAY),
    easy: make(7 * MS_PER_DAY),
  }
}

/**
 * Minimal Map-backed Storage test double for UI/component/browser tests.
 * Intentionally implements NO FSRS math, learning steps, daily caps, sibling
 * burying, leech thresholds, or stats aggregation — those live in `rem-core`.
 */
export class MemoryStorage implements Storage {
  private readonly listeners = new Set<() => void>()
  private readonly decks = new Map<ID, Deck>()
  private readonly cards = new Map<ID, Card>()
  private readonly drafts = new Map<ID, CardDraft>()
  private readonly reviewLogs = new Map<ID, ReviewLog>()
  private readonly assets = new Map<ID, Asset>()
  private readonly sessions = new Map<string, StubSession>()
  private seq = 0
  private statsProvider: StatsProvider | null = null
  private todaySummaryOverride: TodaySummary | null = null
  private nextGradeNotice: LeechAction | null = null

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  notifyExternalChange(): void {
    this.notify()
  }

  /** Test helper: seed a ReviewLog row directly (e.g. for DeckSettingsPage optimizer UI). */
  addReviewLog(entry: Omit<ReviewLog, 'id'>): ReviewLog {
    const log: ReviewLog = { id: this.nextId('log'), ...entry }
    this.reviewLogs.set(log.id, log)
    this.notify()
    return log
  }

  /** Test helper: supply fixed or dynamic stats for StatsPage tests. */
  setStats(provider: CollectionStats | StatsProvider | null): void {
    this.statsProvider = typeof provider === 'function' ? provider : provider ? () => provider : null
    this.notify()
  }

  /** Test helper: override TodaySummary returned by getTodaySummary. */
  setTodaySummary(summary: TodaySummary | null): void {
    this.todaySummaryOverride = summary
    this.notify()
  }

  /** Test helper: return a leech notice on the next gradeStudy call. */
  setNextGradeNotice(notice: LeechAction | null): void {
    this.nextGradeNotice = notice
  }

  private ensureAncestorDecks(name: string, kind: SchedulerKind, createdAt: number, now: number): void {
    for (const ancestorName of deckAncestorPaths(name)) {
      const exists = [...this.decks.values()].some((d) => d.name === ancestorName)
      if (exists) continue
      const ancestorId = this.nextId('deck')
      this.decks.set(ancestorId, {
        id: ancestorId,
        name: ancestorName,
        createdAt,
        updatedAt: now,
        color: deckColor(ancestorId),
        schedulerKind: kind,
        settings: { ...DEFAULT_DECK_SETTINGS },
      })
    }
  }

  async createDeck(name: string, kind: SchedulerKind = 'fsrs'): Promise<Deck> {
    const now = Date.now()
    const normalized = normalizeDeckName(name)
    this.ensureAncestorDecks(normalized, kind, now, now)
    const id = this.nextId('deck')
    const deck: Deck = {
      id,
      name: normalized,
      createdAt: now,
      updatedAt: now,
      color: deckColor(id),
      schedulerKind: kind,
      settings: { ...DEFAULT_DECK_SETTINGS },
    }
    this.decks.set(id, deck)
    this.notify()
    return deck
  }

  async listDecks(): Promise<Deck[]> {
    return [...this.decks.values()]
  }

  async getDeck(id: ID): Promise<Deck | undefined> {
    return this.decks.get(id)
  }

  async updateDeck(id: ID, patch: DeckPatch): Promise<void> {
    const existing = this.decks.get(id)
    if (!existing) return
    const now = Date.now()
    const nextName = patch.name !== undefined ? normalizeDeckName(patch.name) : existing.name
    this.decks.set(id, { ...existing, ...patch, name: nextName, updatedAt: now })
    if (nextName !== existing.name) {
      const prefix = `${existing.name}::`
      for (const child of this.decks.values()) {
        if (child.id !== id && child.name.startsWith(prefix)) {
          this.decks.set(child.id, {
            ...child,
            name: `${nextName}::${child.name.slice(prefix.length)}`,
            updatedAt: now,
          })
        }
      }
      this.ensureAncestorDecks(nextName, existing.schedulerKind, now, now)
    }
    this.notify()
  }

  async deleteDeck(id: ID): Promise<void> {
    const existing = this.decks.get(id)
    const ids = existing
      ? [...this.decks.values()]
          .filter((d) => d.id === id || deckIsStrictDescendant(d.name, existing.name))
          .map((d) => d.id)
      : [id]
    this.removeDeckIds(ids)
    this.notify()
  }

  async deleteDeckTree(path: string): Promise<ID[]> {
    const normalized = normalizeDeckName(path)
    const ids = [...this.decks.values()]
      .filter((d) => deckIsDescendantOrSelf(d.name, normalized))
      .map((d) => d.id)
    if (ids.length === 0) throw new Error(`deck not found: ${normalized}`)
    this.removeDeckIds(ids)
    this.notify()
    return ids
  }

  async createCard(deckId: ID, front: string, back: string, tags: string[] = []): Promise<Card> {
    if (!front.trim()) throw new Error('card front must not be blank')
    if (!this.decks.has(deckId)) throw new Error(`deck not found: ${deckId}`)
    const now = Date.now()
    const card: Card = {
      id: this.nextId('card'),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      tags: [...tags],
      suspended: false,
      lastAgainAt: null,
      scheduling: initialFsrsState(now),
    }
    this.cards.set(card.id, card)
    this.notify()
    return card
  }

  async createCardOrCloze(
    deckId: ID,
    front: string,
    back: string,
    tags: string[] = [],
  ): Promise<Card[]> {
    const indices = extractSiblingCardIndices(front)
    if (indices.length === 0) return [await this.createCard(deckId, front, back, tags)]
    if (!this.decks.has(deckId)) throw new Error(`deck not found: ${deckId}`)
    const now = Date.now()
    const siblingGroupId = this.nextId('sib')
    const created = indices.map((clozeIndex): Card => ({
      id: this.nextId('card'),
      deckId,
      front,
      back,
      createdAt: now,
      updatedAt: now,
      tags: [...tags],
      suspended: false,
      lastAgainAt: null,
      scheduling: initialFsrsState(now),
      siblingGroupId,
      clozeIndex,
    }))
    for (const card of created) this.cards.set(card.id, card)
    this.notify()
    return created
  }

  async updateClozeNote(
    cardId: ID,
    front: string,
    back: string,
    tags: string[] = [],
  ): Promise<Card[]> {
    const target = this.cards.get(cardId)
    if (!target) throw new Error(`card not found: ${cardId}`)
    const now = Date.now()
    const affected: Card[] = []
    const group = target.siblingGroupId
      ? [...this.cards.values()].filter((c) => c.siblingGroupId === target.siblingGroupId)
      : [target]
    for (const card of group) {
      const nextTags = card.tags.includes('leech') && !tags.includes('leech')
        ? ['leech', ...tags]
        : [...tags]
      const updated = { ...card, front, back, tags: nextTags, updatedAt: now }
      this.cards.set(card.id, updated)
      affected.push(updated)
    }
    this.notify()
    return affected
  }

  async getCard(id: ID): Promise<Card | undefined> {
    return this.cards.get(id)
  }

  async listCards(deckId: ID): Promise<Card[]> {
    return [...this.cards.values()].filter((c) => c.deckId === deckId)
  }

  async searchCards(
    query: string,
    options?: { deckId?: ID; limit?: number },
  ): Promise<CardSearchHit[]> {
    const tokens = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean)
    if (tokens.length === 0) return []
    const limit = Math.min(options?.limit ?? 50, 200)
    const hits: CardSearchHit[] = []
    for (const card of this.cards.values()) {
      if (options?.deckId && card.deckId !== options.deckId) continue
      const deck = this.decks.get(card.deckId)
      if (!deck) continue
      const hay = [card.front, card.back, card.tags.join(' '), card.rationale ?? '']
        .join('\n')
        .toLocaleLowerCase()
      if (tokens.every((t) => hay.includes(t))) {
        hits.push({ card, deckName: deck.name, deckColor: deck.color })
      }
    }
    hits.sort((a, b) => b.card.updatedAt - a.card.updatedAt)
    return hits.slice(0, limit)
  }

  async updateCard(id: ID, patch: CardPatch): Promise<void> {
    const existing = this.cards.get(id)
    if (!existing) return
    this.cards.set(id, { ...existing, ...patch, updatedAt: Date.now() })
    this.notify()
  }

  async deleteCard(id: ID): Promise<void> {
    this.cards.delete(id)
    for (const [logId, log] of this.reviewLogs) {
      if (log.cardId === id) this.reviewLogs.delete(logId)
    }
    this.notify()
  }

  async proposeDrafts(
    deckId: ID,
    inputs: NewDraftInput[],
    metadata: ProposalMetadata,
    dryRun = false,
  ): Promise<ProposeDraftsResult> {
    if (!this.decks.has(deckId)) throw new Error(`deck not found: ${deckId}`)
    const now = Date.now()
    const outcomes: DraftProposalOutcome[] = []
    for (const input of inputs) {
      const draft: CardDraft = {
        id: this.nextId('draft'),
        deckId,
        front: input.front,
        back: input.back,
        tags: [...input.tags],
        rationale: input.rationale?.trim() || null,
        sources: input.sources.map((s) => ({ locator: s.locator.trim(), label: s.label?.trim() || null })),
        proposedBy: metadata.proposedBy?.trim() || null,
        createdAt: now,
        updatedAt: now,
        revision: 0,
      }
      if (!dryRun) this.drafts.set(draft.id, draft)
      outcomes.push({ status: 'created', value: draft })
    }
    if (!dryRun && outcomes.length > 0) this.notify()
    return { outcomes }
  }

  async listDrafts(): Promise<CardDraft[]> {
    return [...this.drafts.values()]
  }

  async resolveDraft(
    id: ID,
    expectedRevision: number,
    decision: DraftDecision,
  ): Promise<DraftResolution> {
    const draft = this.drafts.get(id)
    if (!draft) throw new Error(`draft not found: ${id}`)
    if (draft.revision !== expectedRevision) throw new Error(`draft changed: ${id}`)
    this.drafts.delete(id)
    if (decision.decision === 'reject') {
      this.notify()
      return { status: 'rejected' }
    }
    if (!this.decks.has(decision.deckId)) throw new Error(`deck not found: ${decision.deckId}`)
    const existing = [...this.cards.values()].find(
      (c) =>
        c.deckId === decision.deckId &&
        c.front === decision.card.front &&
        c.back === decision.card.back &&
        c.tags.join('\0') === decision.card.tags.join('\0'),
    )
    if (existing) {
      this.notify()
      return { status: 'existingCard', value: existing }
    }
    const now = Date.now()
    const card: Card = {
      id: this.nextId('card'),
      deckId: decision.deckId,
      front: decision.card.front,
      back: decision.card.back,
      createdAt: now,
      updatedAt: now,
      tags: [...decision.card.tags],
      suspended: false,
      lastAgainAt: null,
      scheduling: initialFsrsState(now),
      rationale: draft.rationale,
      sources: draft.sources,
      proposedBy: draft.proposedBy,
    }
    this.cards.set(card.id, card)
    this.notify()
    return { status: 'accepted', value: card }
  }

  async startStudy(request: StudyRequest): Promise<StartedStudy> {
    const now = Date.now()
    const rootDeck = request.deckId ? this.decks.get(request.deckId) : undefined
    const targetIds = request.deckId
      ? new Set(
          rootDeck
            ? [...this.decks.values()]
                .filter((d) => d.id === rootDeck.id || deckIsStrictDescendant(d.name, rootDeck.name))
                .map((d) => d.id)
            : [],
        )
      : null
    const queue = [...this.cards.values()].filter((c) => {
      if (c.suspended) return false
      if (targetIds && !targetIds.has(c.deckId)) return false
      return request.custom !== null ? true : c.scheduling.due <= now
    })
    const current = queue.shift() ?? null
    const session: StubSession = {
      queue,
      current,
      revealed: false,
      nextStates: null,
      reviewed: 0,
      preview: request.custom?.mode === 'preview-new',
      showTimer: current ? (this.decks.get(current.deckId)?.settings.showTimer ?? false) : false,
      notice: null,
      lastUndo: null,
    }
    const sessionId = this.nextId('study')
    this.sessions.set(sessionId, session)
    return { sessionId, view: this.viewOf(session) }
  }

  async revealStudy(sessionId: string): Promise<StudyView> {
    const session = this.getSession(sessionId)
    if (!session.current) throw new Error('study session has no current card')
    if (!session.revealed) {
      session.revealed = true
      if (!session.preview) {
        session.nextStates = cannedNextStates(session.current, Date.now())
      }
    }
    return this.viewOf(session)
  }

  async gradeStudy(sessionId: string, grade: Grade): Promise<StudyGradeOutcome> {
    const session = this.getSession(sessionId)
    if (!session.current || !session.nextStates) {
      throw new Error('study card must be revealed before grading')
    }
    const prev = structuredClone(session.current)
    const nextSched = session.nextStates[grade]
    this.cards.set(prev.id, { ...prev, scheduling: nextSched, updatedAt: Date.now() })
    session.lastUndo = { card: prev, previousNotice: session.notice }
    session.reviewed += 1
    session.current = session.queue.shift() ?? null
    session.revealed = false
    session.nextStates = null
    session.notice = this.nextGradeNotice
    this.nextGradeNotice = null
    this.notify()
    return { status: 'graded', view: this.viewOf(session) }
  }

  async advanceStudyPreview(sessionId: string): Promise<StudyView> {
    const session = this.getSession(sessionId)
    session.reviewed += 1
    session.current = session.queue.shift() ?? null
    session.revealed = false
    session.nextStates = null
    return this.viewOf(session)
  }

  async undoStudyGrade(sessionId: string): Promise<StudyView> {
    const session = this.getSession(sessionId)
    if (!session.lastUndo) throw new Error('no recent grade to undo')
    const { card, previousNotice } = session.lastUndo
    this.cards.set(card.id, card)
    if (session.current) session.queue.unshift(session.current)
    session.current = card
    session.reviewed = Math.max(0, session.reviewed - 1)
    session.revealed = false
    session.nextStates = null
    session.notice = previousNotice
    session.lastUndo = null
    this.notify()
    return this.viewOf(session)
  }

  async suspendStudyCard(sessionId: string): Promise<StudyView> {
    const session = this.getSession(sessionId)
    if (!session.current) throw new Error('study session has no current card')
    const cur = this.cards.get(session.current.id)
    if (cur) this.cards.set(cur.id, { ...cur, suspended: true, updatedAt: Date.now() })
    session.lastUndo = null
    session.current = session.queue.shift() ?? null
    session.revealed = false
    session.nextStates = null
    session.notice = null
    this.notify()
    return this.viewOf(session)
  }

  async endStudy(sessionId: string): Promise<void> {
    this.sessions.delete(sessionId)
  }

  async listReviewLogs(deckId: ID): Promise<ReviewLog[]> {
    return [...this.reviewLogs.values()]
      .filter((l) => l.deckId === deckId)
      .sort((a, b) => a.reviewedAt - b.reviewedAt)
  }

  async getTodaySummary(now = Date.now()): Promise<TodaySummary> {
    if (this.todaySummaryOverride) return this.todaySummaryOverride
    let totalDue = 0
    let newDue = 0
    const decks: DeckTodaySummary[] = [...this.decks.values()].map((deck) => {
      const cards = [...this.cards.values()].filter((c) => c.deckId === deck.id)
      const dueCards = cards.filter((c) => !c.suspended && c.scheduling.due <= now)
      const newDueCount = dueCards.filter((c) => c.scheduling.state === FSRS_STATE.New).length
      totalDue += dueCards.length
      newDue += newDueCount
      return {
        id: deck.id,
        name: deck.name,
        color: deck.color,
        totalCards: cards.length,
        dueCount: dueCards.length,
        newDueCount,
        masteredRatio: 0,
        desiredRetention: deck.settings.desiredRetention,
      }
    })
    return {
      totalDue,
      newDue,
      reviewDue: totalDue - newDue,
      estimatedMinutes: Math.ceil((totalDue * 10) / 60),
      reviewedToday: 0,
      recallTodayPercent: null,
      currentStreakDays: 0,
      decks,
    }
  }

  async getStats(deckId: ID | null, days: number, now = Date.now()): Promise<CollectionStats> {
    return this.statsProvider ? this.statsProvider(deckId, days, now) : emptyStats()
  }

  async importDecks(incoming: DeckBackup[]): Promise<ImportResult> {
    const normalized = incoming.map((d) => ({ ...d, name: normalizeDeckName(d.name) }))
    const existing = [...this.decks.values()]
    const result = planImport(
      normalized.map((d) => d.name),
      existing.map((d) => d.name),
    )
    const toReplace = new Set(result.replaced)
    this.removeDeckIds(existing.filter((d) => toReplace.has(d.name)).map((d) => d.id))
    const now = Date.now()
    for (const d of normalized) {
      const deckId = this.nextId('deck')
      this.decks.set(deckId, {
        id: deckId,
        name: d.name,
        createdAt: d.createdAt,
        updatedAt: now,
        color: d.color ?? deckColor(deckId),
        schedulerKind: d.schedulerKind,
        settings: d.settings,
      })
      for (const c of d.cards) {
        const cardId = this.nextId('card')
        this.cards.set(cardId, {
          id: cardId,
          deckId,
          front: c.front,
          back: c.back,
          createdAt: c.createdAt,
          updatedAt: c.updatedAt,
          tags: [...c.tags],
          suspended: c.suspended,
          lastAgainAt: c.lastAgainAt,
          scheduling: c.scheduling,
          rationale: c.rationale ?? null,
          sources: c.sources ?? [],
          proposedBy: c.proposedBy ?? null,
          siblingGroupId: c.siblingGroupId ?? null,
          clozeIndex: c.clozeIndex ?? null,
        })
        for (const r of c.reviews) {
          const logId = this.nextId('log')
          this.reviewLogs.set(logId, { id: logId, deckId, cardId, reviewedAt: r.reviewedAt, grade: r.grade })
        }
      }
    }
    for (const d of normalized) {
      this.ensureAncestorDecks(d.name, d.schedulerKind, Math.max(0, d.createdAt - 1), now)
    }
    this.notify()
    return result
  }

  async putAsset(bytes: Uint8Array, mime: string): Promise<Asset> {
    const hash = this.syntheticHash(bytes)
    const existing = this.assets.get(hash)
    if (existing) return existing
    const asset: Asset = { hash, mime, bytes: new Uint8Array(bytes), createdAt: Date.now() }
    this.assets.set(hash, asset)
    this.notify()
    return asset
  }

  async getAsset(hash: ID): Promise<Asset | undefined> {
    return this.assets.get(hash)
  }

  async sweepOrphanAssets(): Promise<void> {}

  private removeDeckIds(ids: ID[]): void {
    const set = new Set(ids)
    for (const id of set) this.decks.delete(id)
    for (const [id, c] of this.cards) if (set.has(c.deckId)) this.cards.delete(id)
    for (const [id, d] of this.drafts) if (set.has(d.deckId)) this.drafts.delete(id)
    for (const [id, l] of this.reviewLogs) if (set.has(l.deckId)) this.reviewLogs.delete(id)
  }

  private getSession(sessionId: string): StubSession {
    const session = this.sessions.get(sessionId)
    if (!session) throw new Error(`study session not found: ${sessionId}`)
    return session
  }

  private viewOf(session: StubSession): StudyView {
    return {
      current: session.current,
      revealed: session.revealed,
      nextStates: session.nextStates,
      reviewed: session.reviewed,
      remaining: session.queue.length + (session.current ? 1 : 0),
      preview: session.preview,
      canUndo: session.lastUndo !== null,
      showTimer: session.showTimer,
      notice: session.notice,
    }
  }

  private nextId(prefix: string): string {
    this.seq += 1
    return `${prefix}-${this.seq}`
  }

  private syntheticHash(bytes: Uint8Array): string {
    let h1 = 0x811c9dc5
    let h2 = 0x9e3779b9
    for (let i = 0; i < bytes.length; i++) {
      h1 = Math.imul(h1 ^ bytes[i], 0x01000193) >>> 0
      h2 = Math.imul(h2 ^ (bytes[i] + i), 0x85ebca6b) >>> 0
    }
    const seed = `${h1.toString(16).padStart(8, '0')}${h2.toString(16).padStart(8, '0')}${bytes.length.toString(16).padStart(8, '0')}`
    return seed.padEnd(64, '0').slice(0, 64)
  }

  private notify(): void {
    for (const listener of this.listeners) listener()
  }
}
