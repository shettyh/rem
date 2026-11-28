import type { CustomStudyMode, CustomStudyRequest } from '../../domain/models'

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
  const amount = Number.isFinite(parsed)
    ? Math.min(999, Math.max(1, Math.floor(parsed)))
    : preset.defaultAmount
  return { mode: preset.mode, amount }
}
