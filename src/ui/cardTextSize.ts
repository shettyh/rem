export type CardTextSize = 'compact' | 'standard' | 'large'

export const CARD_TEXT_SIZE_KEY = 'rem-card-text-size'

export function getStoredCardTextSize(): CardTextSize {
  if (typeof localStorage === 'undefined') return 'standard'
  const stored = localStorage.getItem(CARD_TEXT_SIZE_KEY)
  if (stored === 'compact' || stored === 'standard' || stored === 'large') return stored
  return 'standard'
}

export function applyCardTextSize(size: CardTextSize): void {
  if (size === 'standard') {
    delete document.documentElement.dataset.cardSize
  } else {
    document.documentElement.dataset.cardSize = size
  }
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(CARD_TEXT_SIZE_KEY, size)
  }
}
