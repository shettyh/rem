import type { Card, DeckSettings, FSRSState, Grade, LeechAction } from '../../domain/models'

export interface LeechEffect {
  action: LeechAction
  tags: string[]
  suspended: boolean
}

/** Durable metadata effect of a grade that newly crosses the deck's leech threshold. */
export function leechEffect(
  card: Pick<Card, 'scheduling' | 'tags' | 'suspended'>,
  settings: Pick<DeckSettings, 'leechThreshold' | 'leechAction'>,
  grade: Grade,
  next: FSRSState,
): LeechEffect | null {
  if (
    grade !== 'again' ||
    card.scheduling.state !== 2 ||
    next.lapses <= card.scheduling.lapses ||
    next.lapses < settings.leechThreshold ||
    card.tags.includes('leech')
  ) {
    return null
  }

  return {
    action: settings.leechAction,
    tags: [...card.tags, 'leech'],
    suspended: settings.leechAction === 'suspend' ? true : card.suspended,
  }
}
