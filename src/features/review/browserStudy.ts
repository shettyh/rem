import type {
  CardPatch,
  Storage,
} from '../../data/Storage'
import {
  FSRS_STATE,
  type DeckSettings,
  type Grade,
  type LeechAction,
  type StudyGradeOutcome,
  type StudyRequest,
  type StudyView,
} from '../../domain/models'
import { nextStates } from '../../domain/scheduler/reviewScheduler'
import {
  selectCustomStudyCards,
} from './customStudy'
import { localDay } from './day'
import { loadDueOverview, shuffle } from './dueOverview'
import { leechEffect } from './leech'
import { buildSessionCards, ReviewSession, type SessionCard } from './session'

export interface ReviewRollback {
  cardId: string
  deckId: string
  patch: CardPatch
  reviewLogId?: string
  daily?: { day: string; field: 'newIntroduced' | 'reviewsDone' }
}

interface LastGradeSnapshot {
  previousItem: SessionCard
  previousNotice: LeechAction | null
  reviewLogId?: string
  daily?: { day: string; field: 'newIntroduced' | 'reviewsDone' }
}

/** Browser-test adapter for the native StudySession interface. */
export class BrowserStudySession {
  private constructor(
    private readonly storage: Storage,
    private readonly session: ReviewSession,
    private current: SessionCard | null,
    private readonly preview: boolean,
    private readonly rollbackReview?: (rollback: ReviewRollback) => Promise<void>,
  ) {}

  private revealed = false
  private choices: StudyView['nextStates'] = null
  private notice: LeechAction | null = null
  private lastGrade: LastGradeSnapshot | null = null

  static async start(
    storage: Storage,
    request: StudyRequest,
    now: number,
    rollbackReview?: (rollback: ReviewRollback) => Promise<void>,
  ): Promise<BrowserStudySession> {
    const cards = await buildInitialCards(storage, request, now)
    const session = new ReviewSession(cards)
    return new BrowserStudySession(
      storage,
      session,
      session.next(now),
      request.custom?.mode === 'preview-new',
      rollbackReview,
    )
  }

  view(): StudyView {
    return {
      current: this.current?.card ?? null,
      revealed: this.revealed,
      nextStates: this.choices,
      reviewed: this.session.reviewed,
      remaining: this.session.remaining,
      preview: this.preview,
      canUndo: this.lastGrade !== null,
      notice: this.notice,
    }
  }

  async reveal(now: number): Promise<StudyView> {
    if (this.revealed) return this.view()
    if (!this.current) throw new Error('invalid input: study session has no current card')
    if (!this.preview) {
      this.choices = await nextStates(this.current.card.scheduling, this.current.settings, now)
    }
    this.revealed = true
    return this.view()
  }

  async grade(grade: Grade, now: number): Promise<StudyGradeOutcome> {
    if (this.preview) throw new Error('invalid input: preview cards cannot be graded')
    if (!this.current || !this.choices || !this.revealed) {
      throw new Error('invalid input: study card must be revealed before grading')
    }
    const current = this.current
    const next = this.choices[grade]
    const effect = leechEffect(current.card, current.settings, grade, next)
    const patch: CardPatch = { scheduling: next }
    if (effect) {
      patch.tags = effect.tags
      patch.suspended = effect.suspended
    }
    if (grade === 'again') patch.lastAgainAt = now
    const preState = current.card.scheduling.state
    const daily = preState === FSRS_STATE.New
      ? { day: localDay(now), field: 'newIntroduced' as const }
      : preState === FSRS_STATE.Review
        ? { day: localDay(now), field: 'reviewsDone' as const }
        : undefined
    const log = await this.storage.commitReview({
      cardId: current.card.id,
      deckId: current.card.deckId,
      patch,
      reviewedAt: now,
      fsrsGrade: next.reps > current.card.scheduling.reps ? grade : undefined,
      daily,
    })
    const previousNotice = this.notice
    this.lastGrade = {
      previousItem: {
        ...current,
        card: {
          ...current.card,
          tags: [...current.card.tags],
          scheduling: { ...current.card.scheduling },
        },
      },
      previousNotice,
      reviewLogId: log?.id,
      daily,
    }
    this.session.grade(now, next, { requeue: effect?.suspended !== true })
    this.current = this.session.next(now)
    this.revealed = false
    this.choices = null
    this.notice = effect?.action ?? null
    return { status: 'graded', view: this.view() }
  }

  advancePreview(now: number): StudyView {
    if (!this.preview) throw new Error('invalid input: only preview sessions can advance without grading')
    if (!this.revealed) throw new Error('invalid input: preview card must be revealed before advancing')
    this.session.complete()
    this.current = this.session.next(now)
    this.revealed = false
    return this.view()
  }

  async undoLastGrade(): Promise<StudyView> {
    const undo = this.lastGrade
    if (!undo) throw new Error('invalid input: no recent grade to undo')
    this.lastGrade = null
    const prev = undo.previousItem.card
    const restorePatch: CardPatch = {
      tags: [...prev.tags],
      suspended: prev.suspended,
      lastAgainAt: prev.lastAgainAt,
      scheduling: { ...prev.scheduling },
    }
    if (this.rollbackReview) {
      await this.rollbackReview({
        cardId: prev.id,
        deckId: prev.deckId,
        patch: restorePatch,
        reviewLogId: undo.reviewLogId,
        daily: undo.daily,
      })
    } else {
      await this.storage.updateCard(prev.id, restorePatch)
    }
    const reloaded = (await this.storage.getCard(prev.id)) ?? prev
    const restoredItem: SessionCard = {
      ...undo.previousItem,
      card: reloaded,
      forceDue: true,
    }
    this.session.restoreGraded(restoredItem)
    this.current = restoredItem
    this.revealed = false
    this.choices = null
    this.notice = undo.previousNotice
    return this.view()
  }

  async suspendCurrent(now: number): Promise<StudyView> {
    if (this.preview) throw new Error('invalid input: preview cards cannot be suspended')
    if (!this.current) throw new Error('invalid input: study session has no current card')
    const cardId = this.current.card.id
    await this.storage.updateCard(cardId, { suspended: true })
    this.lastGrade = null
    this.session.discardCurrent(cardId)
    this.current = this.session.next(now)
    this.revealed = false
    this.choices = null
    this.notice = null
    return this.view()
  }
}

async function buildInitialCards(
  storage: Storage,
  request: StudyRequest,
  now: number,
): Promise<SessionCard[]> {
  if (request.deckId) {
    const deck = await storage.getDeck(request.deckId)
    if (!deck) return []
    const stat = await storage.getDailyStat(request.deckId, localDay(now))
    const newSlots = deck.settings.newPerDay - stat.newIntroduced
    if (request.custom) {
      const cards = await storage.listCards(request.deckId)
      return selectCustomStudyCards(cards, request.custom, now, {
        insertionOrder: deck.settings.insertionOrder,
        normalNewSlots: newSlots,
      }).map((card) => ({ card, settings: deck.settings, forceDue: true }))
    }
    const due = await storage.dueCards(request.deckId, now)
    return buildSessionCards(
      due.map((card) => ({ card, settings: deck.settings })),
      deck.settings.insertionOrder,
      {
        newSlots,
        reviewSlots: deck.settings.maxReviews - stat.reviewsDone,
      },
    )
  }

  if (request.custom) throw new Error('invalid input: custom study requires one deck')
  const overview = await loadDueOverview(storage, now)
  const settings = new Map<string, DeckSettings>(
    overview.decks.map(({ deck }) => [deck.id, deck.settings]),
  )
  return shuffle(overview.queue).map((card) => ({ card, settings: settings.get(card.deckId)! }))
}
