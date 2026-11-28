import { describe, expect, it } from 'vitest'
import Dexie from 'dexie'
import { RemDB } from '../../data/dexie/db'
import { DexieStorage } from '../../data/dexie/DexieStorage'
import { DEFAULT_DECK_SETTINGS, FSRS_STATE, type DeckSettings, type FSRSState, type Grade } from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'
import { nextStates } from '../../domain/scheduler/reviewScheduler'
import { parseStepsMs } from '../../domain/scheduler/steps'
import { forAll, forAllAsync, pick, randBool, randInt } from '../../test/prop'
import { BrowserStudySession } from './browserStudy'
import { localDay } from './day'
import { LEARN_AHEAD_MS } from './session'

const GRADES: readonly Grade[] = ['again', 'hard', 'good', 'easy']

describe('steps & nextStates scheduler invariants', () => {
  it('parseStepsMs rejects zero, negative, non-numeric, and overflowing durations', () => {
    expect(parseStepsMs('0s 0m 0h 0d -5m -10 10x 9999999999999999d 100000d')).toEqual([])
    expect(parseStepsMs('1s 1m 10 2h 3d')).toEqual([1000, 60_000, 600_000, 7_200_000, 3 * MS_PER_DAY])
  })

  it('property: nextStates always yields finite, monotonic-rep, valid-state, due >= now outcomes', async () => {
    await forAllAsync(120, 0x5c8ed1, async (rng) => {
      const now = 1_700_000_000_000 + randInt(rng, 0, 365 * MS_PER_DAY)
      const stateNum = pick(rng, [0, 1, 2, 3])
      const reps = stateNum === 0 ? 0 : randInt(rng, 0, 20)
      const scheduling: FSRSState = {
        kind: 'fsrs',
        stability: reps === 0 ? 0 : 1 + rng() * 30,
        difficulty: reps === 0 ? 0 : 1 + rng() * 9,
        reps,
        lapses: randInt(rng, 0, 10),
        state: stateNum,
        step: randInt(rng, 0, 5),
        lastReview: reps === 0 ? null : now - randInt(rng, 0, 10 * MS_PER_DAY),
        due: now - randInt(rng, 0, 5 * MS_PER_DAY),
      }
      const settings: DeckSettings = {
        ...DEFAULT_DECK_SETTINGS,
        learnSteps: pick(rng, ['', '0m', '-5m', '1m', '1m 10m', '30s 5m 1h', '10m foo 0s']),
        relearnSteps: pick(rng, ['', '0m', '-1m', '10m', '5m 15m']),
        minimumInterval: pick(rng, [-2, 0, 1, 3, 10, 50]),
        maximumInterval: pick(rng, [-1, 0, 1, 5, 30, 36500]),
      }

      const ns = await nextStates(scheduling, settings, now)
      for (const grade of GRADES) {
        const out = ns[grade]
        expect(Number.isFinite(out.due)).toBe(true)
        expect(out.due).toBeGreaterThanOrEqual(now)
        expect([0, 1, 2, 3]).toContain(out.state)
        expect(Number.isInteger(out.step) && out.step >= 0).toBe(true)
        expect(out.reps).toBeGreaterThanOrEqual(scheduling.reps)
        expect(out.lapses).toBeGreaterThanOrEqual(scheduling.lapses)
        // Step cards must always schedule strictly in the future (no 0ms infinite loop).
        if (out.state === 1 || out.state === 3) {
          expect(out.due).toBeGreaterThan(now)
        }
      }
    })
  })
})

describe('BrowserStudySession deterministic state-machine invariants', () => {
  it('property: random reveal/grade/undo/suspend/clock sequences preserve storage, caps, sibling burying, and undo exactness', async () => {
    await forAllAsync(20, 0x57a7e1, async (rng, iter) => {
      const dbName = `study-sm-${iter}`
      await Dexie.delete(dbName)
      const db = new RemDB(dbName)
      const storage = new DexieStorage(db)
      try {
        const baseNow = new Date(2026, 6, 15, 10, 0, 0, 0).getTime()
        let now = baseNow

        const deck = await storage.createDeck('SM Deck')
        await storage.updateDeck(deck.id, {
          settings: {
            ...DEFAULT_DECK_SETTINGS,
            learnSteps: pick(rng, ['', '1m', '1m 10m']),
            relearnSteps: pick(rng, ['', '5m']),
            leechThreshold: randInt(rng, 1, 3),
            leechAction: pick(rng, ['tag', 'suspend'] as const),
            buryRelated: randBool(rng, 0.7),
            newPerDay: randInt(rng, 1, 5),
            maxReviews: randInt(rng, 1, 5),
          },
        })

        // Seed mix of new, review, and cloze sibling cards.
        const singleCount = randInt(rng, 2, 4)
        for (let i = 0; i < singleCount; i++) {
          const card = await storage.createCard(deck.id, `Q${i}`, `A${i}`)
          if (randBool(rng)) {
            await storage.updateCard(card.id, {
              scheduling: {
                kind: 'fsrs',
                stability: 5,
                difficulty: 5,
                reps: 2,
                lapses: randInt(rng, 0, 2),
                state: FSRS_STATE.Review,
                step: 0,
                lastReview: baseNow - 2 * MS_PER_DAY,
                due: baseNow - 1000,
              },
            })
          } else {
            await storage.updateCard(card.id, {
              scheduling: { ...card.scheduling, due: baseNow - 1000 },
            })
          }
        }
        const siblings = await storage.createCardOrCloze(
          deck.id,
          'Siblings {{c1::one}} and {{c2::two}} and {{c3::three}}',
          'Extra',
        )
        for (const sib of siblings) {
          await storage.updateCard(sib.id, {
            scheduling: { ...sib.scheduling, due: baseNow - 1000 },
          })
        }

        const session = await BrowserStudySession.start(
          storage,
          { deckId: deck.id, custom: null },
          now,
          async (rollback) => {
            await db.transaction('rw', db.cards, db.dailyStats, db.reviewLogs, async () => {
              await db.cards.update(rollback.cardId, { ...rollback.patch, updatedAt: now })
              if (rollback.reviewLogId) await db.reviewLogs.delete(rollback.reviewLogId)
              if (rollback.daily) {
                const statId = `${rollback.deckId}:${rollback.daily.day}`
                const row = await db.dailyStats.get(statId)
                if (row) {
                  const field = rollback.daily.field
                  await db.dailyStats.put({ ...row, [field]: Math.max(0, row[field] - 1) })
                }
              }
            })
          },
        )

        const deckSettings = (await storage.getDeck(deck.id))!.settings
        const gradedSiblingGroupsToday = new Map<string, string>()

        for (let step = 0; step < 25; step++) {
          const view = session.view()
          expect(view.remaining).toBeGreaterThanOrEqual(0)
          expect(view.reviewed).toBeGreaterThanOrEqual(0)

          if (!view.current) {
            // If session finished, occasionally undo from completion!
            if (view.canUndo && randBool(rng, 0.5)) {
              const undone = await session.undoLastGrade()
              expect(undone.current).not.toBeNull()
              expect(undone.canUndo).toBe(false)
              continue
            }
            break
          }

          // Advance clock occasionally (within learn-ahead or slightly past).
          if (randBool(rng, 0.3)) {
            now += pick(rng, [30_000, 5 * 60_000, LEARN_AHEAD_MS])
          }

          const action = pick(rng, ['grade', 'grade', 'grade', 'undo', 'suspend'] as const)

          if (action === 'undo' && view.canUndo) {
            const beforeLogs = await storage.listReviewLogs(deck.id)
            const undone = await session.undoLastGrade()
            expect(undone.canUndo).toBe(false)
            expect(undone.revealed).toBe(false)
            expect(undone.current).not.toBeNull()
            const afterLogs = await storage.listReviewLogs(deck.id)
            expect(afterLogs.length).toBeLessThanOrEqual(beforeLogs.length)
            if (undone.current?.siblingGroupId) {
              gradedSiblingGroupsToday.delete(undone.current.siblingGroupId)
            }
            continue
          }

          if (action === 'suspend') {
            const targetId = view.current.id
            const after = await session.suspendCurrent(now)
            expect(after.canUndo).toBe(false)
            expect((await storage.getCard(targetId))?.suspended).toBe(true)
            continue
          }

          // Check sibling-burying invariant before grading a non-learning sibling.
          const cur = view.current
          if (
            deckSettings.buryRelated &&
            cur.siblingGroupId &&
            cur.scheduling.state !== FSRS_STATE.Learning &&
            cur.scheduling.state !== FSRS_STATE.Relearning
          ) {
            const priorCardId = gradedSiblingGroupsToday.get(cur.siblingGroupId)
            if (priorCardId) {
              expect(priorCardId).toBe(cur.id)
            }
          }

          // Capture pre-grade snapshot to verify exact rollback if we undo immediately.
          const preCard = structuredClone((await storage.getCard(cur.id))!)
          const preLogsCount = (await storage.listReviewLogs(deck.id)).length
          const preDaily = await storage.getDailyStat(deck.id, localDay(now))
          const preReviewed = view.reviewed

          const revealed = await session.reveal(now)
          expect(revealed.revealed).toBe(true)
          expect(revealed.nextStates).not.toBeNull()

          const grade = pick(rng, GRADES)
          const outcome = await session.grade(grade, now)
          expect(outcome.status).toBe('graded')
          expect(outcome.view.reviewed).toBe(preReviewed + 1)
          expect(outcome.view.canUndo).toBe(true)

          if (cur.siblingGroupId) {
            gradedSiblingGroupsToday.set(cur.siblingGroupId, cur.id)
          }

          // Immediately test exact undo restoration 35% of the time.
          if (randBool(rng, 0.35)) {
            const restoredView = await session.undoLastGrade()
            expect(restoredView.current?.id).toBe(cur.id)
            expect(restoredView.reviewed).toBe(preReviewed)
            expect(restoredView.canUndo).toBe(false)
            const restoredCard = (await storage.getCard(cur.id))!
            expect(restoredCard.scheduling).toEqual(preCard.scheduling)
            expect(restoredCard.tags).toEqual(preCard.tags)
            expect(restoredCard.suspended).toBe(preCard.suspended)
            expect(restoredCard.lastAgainAt).toBe(preCard.lastAgainAt)
            expect((await storage.listReviewLogs(deck.id)).length).toBe(preLogsCount)
            expect(await storage.getDailyStat(deck.id, localDay(now))).toEqual(preDaily)
            if (cur.siblingGroupId) {
              gradedSiblingGroupsToday.delete(cur.siblingGroupId)
            }
          }
        }
      } finally {
        db.close()
        await Dexie.delete(dbName)
      }
    })
  })

  it('preserves canUndo on transient rollback failure and clears it on conflict/not-found', async () => {
    const dbName = 'study-sm-rollback'
    await Dexie.delete(dbName)
    const db = new RemDB(dbName)
    const storage = new DexieStorage(db)
    try {
      const now = 1_700_000_000_000
      const deck = await storage.createDeck('Rollback')
      const card = await storage.createCard(deck.id, 'Q', 'A')
      await storage.updateCard(card.id, { scheduling: { ...card.scheduling, due: now - 1000 } })

      let failMode: 'transient' | 'conflict' | 'none' = 'transient'
      const session = await BrowserStudySession.start(
        storage,
        { deckId: deck.id, custom: null },
        now,
        async (rollback) => {
          if (failMode === 'transient') throw new Error('temporary I/O error')
          if (failMode === 'conflict') throw new Error('revision conflict')
          await storage.updateCard(rollback.cardId, rollback.patch)
        },
      )

      await session.reveal(now)
      await session.grade('good', now)
      expect(session.view().canUndo).toBe(true)

      await expect(session.undoLastGrade()).rejects.toThrow(/temporary/i)
      expect(session.view().canUndo).toBe(true)

      failMode = 'none'
      const restored = await session.undoLastGrade()
      expect(restored.current?.id).toBe(card.id)
      expect(restored.canUndo).toBe(false)

      await session.reveal(now)
      await session.grade('good', now)
      failMode = 'conflict'
      await expect(session.undoLastGrade()).rejects.toThrow(/conflict/i)
      expect(session.view().canUndo).toBe(false)
    } finally {
      db.close()
      await Dexie.delete(dbName)
    }
  })
})

describe('forAll sanity check', () => {
  it('reproduces deterministic sequences with fixed seed', () => {
    const seen: number[] = []
    forAll(5, 42, (rng) => {
      seen.push(randInt(rng, 0, 1000))
    })
    const again: number[] = []
    forAll(5, 42, (rng) => {
      again.push(randInt(rng, 0, 1000))
    })
    expect(again).toEqual(seen)
  })
})
