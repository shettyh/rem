import type { ReactNode } from 'react'

export type EmptyIconKind = 'decks' | 'cards' | 'done' | 'rest' | 'clear' | 'stats' | 'skip' | 'search'

const ICONS: Record<EmptyIconKind, ReactNode> = {
  decks: (
    <>
      <rect x="3" y="5" width="14" height="11" rx="2" />
      <path d="M6 2.5h9a2 2 0 0 1 2 2V13" />
    </>
  ),
  cards: (
    <>
      <rect x="3" y="4" width="14" height="12" rx="2" />
      <path d="M7 9h6M7 12.5h4" />
    </>
  ),
  done: <path d="M4.5 10.5 8.2 14.2 15.5 6.2" />,
  rest: (
    <>
      <path d="M13.5 11.8A5.5 5.5 0 0 1 7.2 5.5a5.6 5.6 0 1 0 6.3 6.3Z" />
    </>
  ),
  clear: (
    <>
      <path d="M4 5.5A1.5 1.5 0 0 1 5.5 4h9A1.5 1.5 0 0 1 16 5.5v9a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 4 14.5v-9Z" />
      <path d="M4 11h3.5l1.2 1.8h2.6L12.5 11H16" />
    </>
  ),
  stats: <path d="M3.5 16h13M5.5 16v-4.5M10 16V8M14.5 16V4.5" />,
  skip: (
    <>
      <path d="M5 6.5 10 10l-5 3.5v-7Z" />
      <path d="M13.5 6.5v7" />
    </>
  ),
  search: (
    <>
      <circle cx="9" cy="9" r="5" />
      <path d="m13 13 3.5 3.5" />
    </>
  ),
}

export function EmptyIcon({ kind, tone = 'neutral' }: { kind: EmptyIconKind; tone?: 'neutral' | 'success' }) {
  return (
    <div className={tone === 'success' ? 'ico is-success' : 'ico'} aria-hidden="true">
      <svg viewBox="0 0 20 20" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        {ICONS[kind]}
      </svg>
    </div>
  )
}
