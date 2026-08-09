import type {
  CardPatch,
  Storage,
} from '../../data/Storage'
import type {
  DeckSettings,
  Grade,
  LeechAction,
  StudyGradeOutcome,
  StudyRequest,
  StudyView,
} from '../../domain/models'
import { nextStates } from '../../domain/scheduler/reviewScheduler'
import {
  selectCustomStudyCards,
} from './customStudy'
import { localDay } from './day'
import { loadDueOverview, shuffle } from './dueOverview'
import { leechEffect } from './leech'
import { buildSessionCards, ReviewSession, type SessionCard } from './session'

/** Browser-test adapter for the native StudySession interface. */
export class BrowserStudySession {
  private constructor(
    private readonly storage: Storage,
    private readonly session: ReviewSession,
    private current: SessionCard | null,
    private readonly preview: boolean,
  ) {}

  private revealed = false
  private choices: StudyView['nextStates'] = null
  private notice: LeechAction | null = null

  static async start(storage: Storage, request: StudyRequest, now: number): Promise<BrowserStudySession> {
    const cards = await buildInitialCards(storage, request, now)
    const session = new ReviewSession(cards)
    return new BrowserStudySession(
      storage,
      session,
      session.next(now),
      request.custom?.mode === 'preview-new',
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
    const daily = preState === 0
      ? { day: localDay(now), field: 'newIntroduced' as const }
      : preState === 2
        ? { day: localDay(now), field: 'reviewsDone' as const }
        : undefined
    await this.storage.commitReview({
      cardId: current.card.id,
      deckId: current.card.deckId,
      patch,
      reviewedAt: now,
      fsrsGrade: next.reps > current.card.scheduling.reps ? grade : undefined,
      daily,
    })
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
