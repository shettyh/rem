import type { DeckSettings } from '../models'
import type { DeckFsrsParams } from './Scheduler'

/** Per-deck FSRS params from the deck's settings. Weights stay null until #5. */
export function settingsToParams(s: DeckSettings): DeckFsrsParams {
  return { desiredRetention: s.desiredRetention, maximumInterval: s.maximumInterval, weights: null }
}
