import { useEffect, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useStorage } from '../data/StorageContext'
import type { CardSearchHit } from '../domain/models'
import { stripClozeMarkup } from '../features/cards/cloze'

interface SearchModalProps {
  open: boolean
  onClose: () => void
}

function snippet(markdown: string, maxLen = 110): string {
  const plain = stripClozeMarkup(markdown)
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '[image]')
    .replace(/```[\s\S]*?```/g, (block) => block.replace(/```[^\n]*\n?|```/g, ' '))
    .replace(/^\s{0,3}(?:#{1,6}\s+|>\s?)/gm, '')
    .replace(/[*_`~]+/g, '')
    .replace(/\s+/g, ' ')
    .trim()
  if (plain.length <= maxLen) return plain
  return `${plain.slice(0, maxLen - 1).trimEnd()}…`
}

export function SearchModal({ open, onClose }: SearchModalProps) {
  const storage = useStorage()
  const navigate = useNavigate()
  const location = useLocation()
  const dialogRef = useRef<HTMLDivElement | null>(null)
  const inputRef = useRef<HTMLInputElement | null>(null)
  const activeRowRef = useRef<HTMLButtonElement | null>(null)
  const restoreFocusRef = useRef<HTMLElement | null>(null)
  const [query, setQuery] = useState('')
  const [settledQuery, setSettledQuery] = useState('')
  const [hits, setHits] = useState<CardSearchHit[]>([])
  const [loading, setLoading] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const [activeIndex, setActiveIndex] = useState(0)

  useEffect(() => {
    if (!open) {
      setQuery('')
      setSettledQuery('')
      setHits([])
      setSearchError(null)
      setActiveIndex(0)
      const previous = restoreFocusRef.current
      restoreFocusRef.current = null
      if (previous && document.contains(previous)) {
        previous.focus()
      }
      return
    }
    restoreFocusRef.current = document.activeElement as HTMLElement | null
    const raf = requestAnimationFrame(() => inputRef.current?.focus())
    return () => cancelAnimationFrame(raf)
  }, [open])

  // Intercept Escape in capture phase so underlying window listeners (e.g. CardEditorPage, Sheet)
  // do not fire when dismissing the search modal.
  useEffect(() => {
    if (!open) return
    function onWindowKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        onClose()
      } else if (e.key === 'Tab' && dialogRef.current) {
        const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
          'input, button, [href], [tabindex]:not([tabindex="-1"])',
        )
        if (focusable.length === 0) return
        const first = focusable[0]
        const last = focusable[focusable.length - 1]
        const active = document.activeElement as HTMLElement | null
        if (!active || !dialogRef.current.contains(active)) {
          e.preventDefault()
          ;(e.shiftKey ? last : first).focus()
        } else if (e.shiftKey && active === first) {
          e.preventDefault()
          last.focus()
        } else if (!e.shiftKey && active === last) {
          e.preventDefault()
          first.focus()
        }
      }
    }
    window.addEventListener('keydown', onWindowKey, { capture: true })
    return () => window.removeEventListener('keydown', onWindowKey, { capture: true })
  }, [open, onClose])

  useEffect(() => {
    if (!open) return
    const trimmed = query.trim()
    if (!trimmed) {
      setHits([])
      setSettledQuery('')
      setSearchError(null)
      setActiveIndex(0)
      setLoading(false)
      return
    }
    let cancelled = false
    setLoading(true)
    setSearchError(null)
    const timer = window.setTimeout(() => {
      void storage
        .searchCards(trimmed, { limit: 40 })
        .then((results) => {
          if (cancelled) return
          setHits(results)
          setSettledQuery(trimmed)
          setActiveIndex(0)
          setLoading(false)
        })
        .catch((err: unknown) => {
          if (cancelled) return
          console.error('card search failed', err)
          setHits([])
          setSettledQuery(trimmed)
          setSearchError('Search failed. Please try again.')
          setLoading(false)
        })
    }, 60)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [open, query, storage])

  useEffect(() => {
    activeRowRef.current?.scrollIntoView({ block: 'nearest' })
  }, [activeIndex, hits])

  if (!open) return null

  const trimmedQuery = query.trim()
  const resultsCurrent = !loading && settledQuery === trimmedQuery && trimmedQuery.length > 0

  function openHit(hit: CardSearchHit) {
    if (!resultsCurrent) return
    restoreFocusRef.current = null
    onClose()
    navigate(`/decks/${hit.card.deckId}/cards/${hit.card.id}/edit`, {
      state: { returnTo: `${location.pathname}${location.search}` },
    })
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.nativeEvent.isComposing) return
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setActiveIndex((idx) => (hits.length === 0 ? 0 : (idx + 1) % hits.length))
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActiveIndex((idx) => (hits.length === 0 ? 0 : (idx - 1 + hits.length) % hits.length))
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      if (resultsCurrent && hits[activeIndex]) {
        openHit(hits[activeIndex])
      }
    }
  }

  return (
    <div
      className="search-modal-backdrop"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        ref={dialogRef}
        className="search-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Search cards across all decks"
        onKeyDown={handleKeyDown}
      >
        <div className="search-modal-input-row">
          <svg className="search-modal-icon" viewBox="0 0 16 16" aria-hidden="true">
            <circle cx="7" cy="7" r="4.5" />
            <path d="m10.5 10.5 3 3" />
          </svg>
          <input
            ref={inputRef}
            type="search"
            className="search-modal-input"
            placeholder="Search all decks, cards, tags, and rationale…"
            aria-label="Search all cards"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <span className="kbd" aria-hidden="true">esc</span>
        </div>

        <div className="search-modal-body">
          {!query.trim() ? (
            <p className="search-modal-empty">
              Type to search fronts, answers, tags, and draft rationale across every deck.
            </p>
          ) : searchError ? (
            <p className="search-modal-empty" role="alert">{searchError}</p>
          ) : loading && hits.length === 0 ? (
            <p className="search-modal-empty">Searching…</p>
          ) : hits.length === 0 ? (
            <p className="search-modal-empty">No cards match “{query.trim()}”.</p>
          ) : (
            <ul className="search-modal-list" role="listbox" aria-label="Search results">
              {hits.map((hit, index) => {
                const isSelected = index === activeIndex
                return (
                  <li key={hit.card.id} role="presentation">
                    <button
                      ref={isSelected ? activeRowRef : null}
                      type="button"
                      role="option"
                      aria-selected={isSelected}
                      className={`search-result-row${isSelected ? ' is-active' : ''}`}
                      onFocus={() => setActiveIndex(index)}
                      onMouseEnter={() => setActiveIndex(index)}
                      onClick={() => openHit(hit)}
                    >
                      <div className="search-result-top">
                        <span className="search-result-deck">
                          <span
                            className="search-result-dot"
                            style={{ backgroundColor: hit.deckColor }}
                            aria-hidden="true"
                          />
                          {hit.deckName}
                        </span>
                        {hit.card.suspended && (
                          <span className="search-result-pill">Suspended</span>
                        )}
                      </div>
                      <div className="search-result-front">
                        {snippet(hit.card.front, 120) || '(empty front)'}
                      </div>
                      {hit.card.back.trim() && (
                        <div className="search-result-back">
                          {snippet(hit.card.back, 120)}
                        </div>
                      )}
                      {hit.card.tags.length > 0 && (
                        <div className="search-result-tags">
                          {hit.card.tags.slice(0, 5).map((tag) => (
                            <span key={tag} className="search-result-tag">#{tag}</span>
                          ))}
                        </div>
                      )}
                    </button>
                  </li>
                )
              })}
            </ul>
          )}
        </div>
      </div>
    </div>
  )
}
