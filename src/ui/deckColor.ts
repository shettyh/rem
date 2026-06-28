/** Stable accent color per deck, hashed from its id so every surface agrees. */
export const DECK_PALETTE = ['#7e6cff', '#e8638c', '#2fa86b', '#e8922e', '#3ba0e8']

export function deckColor(id: string): string {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (Math.imul(h, 31) + id.charCodeAt(i)) >>> 0
  return DECK_PALETTE[h % DECK_PALETTE.length]
}
