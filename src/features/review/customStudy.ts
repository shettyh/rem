import type {
  Card,
  CustomStudyMode,
  CustomStudyRequest,
  InsertionOrder,
} from '../../domain/models'
import { MS_PER_DAY } from '../../domain/scheduler'

export type { CustomStudyMode, CustomStudyRequest } from '../../domain/models'

export interface CustomStudyPreset {
  mode: CustomStudyMode
  title: string
  description: string
  unit: 'days' | 'cards'
  defaultAmount: number
  step: number
}

export const CUSTOM_STUDY_PRESETS: readonly CustomStudyPreset[] = [
  {
    mode: 'study-ahead',
    title: 'Study ahead',
    description: 'Review cards due later.',
    unit: 'days',
    defaultAmount: 1,
    step: 1,
  },
  {
    mode: 'increase-new',
    title: 'Increase new',
    description: 'More new cards today.',
    unit: 'cards',
    defaultAmount: 10,
    step: 5,
  },
  {
    mode: 'review-forgotten',
    title: 'Review forgotten',
    description: 'Re-see recent lapses.',
    unit: 'days',
    defaultAmount: 1,
    step: 1,
  },
  {
    mode: 'preview-new',
    title: 'Preview new',
    description: 'Peek at upcoming cards.',
    unit: 'days',
    defaultAmount: 1,
    step: 1,
  },
]

export function customStudyPreset(mode: CustomStudyMode): CustomStudyPreset {
  return CUSTOM_STUDY_PRESETS.find((preset) => preset.mode === mode)!
}

export function parseCustomStudyRequest(params: URLSearchParams): CustomStudyRequest | null {
  const mode = params.get('custom')
  const preset = CUSTOM_STUDY_PRESETS.find((candidate) => candidate.mode === mode)
  if (!preset) return null

  const parsed = Number(params.get('amount') ?? preset.defaultAmount)
  const amount = Number.isFinite(parsed) ? Math.min(999, Math.max(1, Math.floor(parsed))) : preset.defaultAmount
  return { mode: preset.mode, amount }
}

interface SelectionOptions {
  insertionOrder: InsertionOrder
  normalNewSlots: number
  rng?: () => number
}

/** Select the initial cards for one temporary custom-study session. */
export function selectCustomStudyCards(
  cards: Card[],
  request: CustomStudyRequest,
  now: number,
  options: SelectionOptions,
): Card[] {
  const active = cards.filter((card) => !card.suspended)

  switch (request.mode) {
    case 'study-ahead': {
      const through = now + request.amount * MS_PER_DAY
      return active
        .filter((card) => card.scheduling.state === 2 && card.scheduling.due > now && card.scheduling.due <= through)
        .sort((a, b) => a.scheduling.due - b.scheduling.due)
    }
    case 'increase-new': {
      const dueNew = active.filter((card) => card.scheduling.state === 0 && card.scheduling.due <= now)
      const ordered = options.insertionOrder === 'random'
        ? shuffle(dueNew, options.rng)
        : dueNew.sort((a, b) => a.createdAt - b.createdAt)
      const skip = Math.max(0, Math.floor(options.normalNewSlots))
      return ordered.slice(skip, skip + request.amount)
    }
    case 'review-forgotten': {
      const since = now - request.amount * MS_PER_DAY
      return active
        .filter((card) => card.lastAgainAt !== null && card.lastAgainAt >= since && card.lastAgainAt <= now)
        .sort((a, b) => b.lastAgainAt! - a.lastAgainAt!)
    }
    case 'preview-new': {
      const since = now - request.amount * MS_PER_DAY
      return active
        .filter((card) => card.scheduling.state === 0 && card.createdAt >= since && card.createdAt <= now)
        .sort((a, b) => b.createdAt - a.createdAt)
    }
  }
}

function shuffle<T>(items: T[], rng: () => number = Math.random): T[] {
  const result = items.slice()
  for (let i = result.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1))
    ;[result[i], result[j]] = [result[j], result[i]]
  }
  return result
}
