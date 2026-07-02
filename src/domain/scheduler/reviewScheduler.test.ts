import { describe, it, expect } from 'vitest'
import { DEFAULT_DECK_SETTINGS } from '../models'
import { settingsToParams } from './reviewScheduler'

describe('settingsToParams', () => {
  it('maps deck settings to FSRS params with null weights', () => {
    const s = { ...DEFAULT_DECK_SETTINGS, desiredRetention: 0.85, maximumInterval: 1000 }
    expect(settingsToParams(s)).toEqual({ desiredRetention: 0.85, maximumInterval: 1000, weights: null })
  })
})
