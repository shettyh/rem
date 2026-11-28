import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useStorage } from '../data/StorageContext'
import type { CardSearchHit, DeckTodaySummary } from '../domain/models'
import { stripClozeMarkup } from '../features/cards/cloze'
import { hasImageOcclusion, stripOcclusionMarkup } from '../features/cards/occlusion'
import { stripRubyMarkup } from '../features/cards/ruby'

interface SearchModalProps {
  open: boolean
  onClose: () => void
}

type SearchOption =
  | { kind: 'deck'; id: string; deck: DeckTodaySummary }
  | { kind: 'card'; id: string; hit: CardSearchHit }

const EMPTY_DECK_LIMIT = 6
const QUERY_DECK_LIMIT = 5

/** Strip markdown/cloze/occlusion formatting into plain single-line text while preserving
 *  intra-word underscores (e.g. `snake_case`) and image alt text. */
function stripMarkdown(markdown: string): string {
  return stripRubyMarkup(stripClozeMarkup(stripOcclusionMarkup(markdown)))
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, (_m, alt: string) => alt.trim() || '[image]')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/(```|~~~)[\s\S]*?\1/g, (block) =>
      block.replace(/^(```|~~~)[^\n]*\n?|(```|~~~)$/g, ' '),
    )
    .replace(/^\s{0,3}(?:#{1,6}\s+|>\s?)/gm, '')
    .replace(/`+/g, '')
    .replace(/(\*\*|__|~~)(.*?)\1/g, '$2')
    .replace(/(^|[^\p{L}\p{N}])([*_])([^\s*_](?:.*?[^\s*_])?)\2(?=[^\p{L}\p{N}]|$)/gu, '$1$3')
    .replace(/\s+/g, ' ')
    .trim()
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function hasCodePunctuation(query: string): boolean {
  return /[^\p{L}\p{N}\s\-'"]/u.test(query)
}

function extractHighlightTerms(query: string): string[] {
  const trimmed = query.trim()
  if (!trimmed) return []
  if (hasCodePunctuation(trimmed)) {
    return [trimmed]
  }
  const rawTokens = trimmed
    .split(/\s+/)
    .map((t) => t.replace(/^["']+|["']+$/g, ''))
    .filter((t) => t.length > 0)
  const expanded: string[] = []
  for (const token of rawTokens) {
    expanded.push(token)
    if (token.includes('-')) {
      for (const part of token.split('-')) {
        if (part.length > 0) expanded.push(part)
      }
    }
  }
  const source = expanded.length > 0 ? expanded : [trimmed]
  return [...new Set(source)].sort((a, b) => b.length - a.length)
}

function extractDeckHighlightTerms(query: string): string[] {
  const trimmed = query.trim()
  if (!trimmed) return []
  if (trimmed.includes('::')) {
    const normalized = trimmed.split('::').join(' / ')
    const parts = trimmed
      .split('::')
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
    return [...new Set([normalized, ...parts])].sort((a, b) => b.length - a.length)
  }
  return extractHighlightTerms(trimmed)
}

function buildHighlightRegex(terms: string[]): RegExp | null {
  if (terms.length === 0) return null
  return new RegExp(`(${terms.map(escapeRegExp).join('|')})`, 'gi')
}

function formatDeckPath(name: string): string {
  return name.split('::').join(' / ')
}

function matchesDeckName(deckName: string, query: string): boolean {
  const trimmed = query.trim()
  if (!trimmed) return true
  const lower = trimmed.toLowerCase()
  const rawLower = deckName.toLowerCase()
  const formattedLower = formatDeckPath(deckName).toLowerCase()
  if (rawLower.includes(lower) || formattedLower.includes(lower)) return true
  if (hasCodePunctuation(trimmed)) return false
  const tokens = trimmed
    .split(/\s+/)
    .map((t) => t.replace(/^["']+|["']+$/g, '').toLowerCase())
    .filter((t) => t.length > 0)
  return tokens.length > 1 && tokens.every((tok) => formattedLower.includes(tok))
}

function textMatchesQuery(text: string, terms: string[]): boolean {
  if (!text || terms.length === 0) return false
  const lower = text.toLowerCase()
  return terms.some((term) => lower.includes(term.toLowerCase()))
}

/** Return a <= maxLen snippet of plain text, centering a window around the first query match
 *  when the match falls past the visible prefix. */
function windowedSnippet(plain: string, terms: string[], maxLen = 120): string {
  if (plain.length <= maxLen) return plain
  if (terms.length > 0) {
    const lower = plain.toLowerCase()
    let matchStart = -1
    let matchLen = 0
    for (const term of terms) {
      const idx = lower.indexOf(term.toLowerCase())
      if (idx !== -1 && (matchStart === -1 || idx < matchStart)) {
        matchStart = idx
        matchLen = term.length
      }
    }
    if (matchStart !== -1 && matchStart + matchLen > maxLen - 8) {
      const lead = Math.max(0, Math.floor((maxLen - matchLen) / 3))
      const start = Math.max(0, matchStart - lead)
      const end = Math.min(plain.length, start + maxLen - (start > 0 ? 1 : 0))
      const prefix = start > 0 ? '…' : ''
      const suffix = end < plain.length ? '…' : ''
      return `${prefix}${plain.slice(start, end).trim()}${suffix}`
    }
  }
  return `${plain.slice(0, maxLen - 1).trimEnd()}…`
}

function renderHighlighted(text: string, regex: RegExp | null): ReactNode {
  if (!text || !regex) return text
  const parts = text.split(regex)
  if (parts.length <= 1) return text
  return parts.map((part, idx) =>
    idx % 2 === 1 ? (
      <mark key={idx} className="search-mark">
        {part}
      </mark>
    ) : (
      <Fragment key={idx}>{part}</Fragment>
    ),
  )
}

function formatDeckMeta(deck: DeckTodaySummary): string {
  const cardLabel = `${deck.totalCards} ${deck.totalCards === 1 ? 'card' : 'cards'}`
  if (deck.dueCount > 0) {
    return `${deck.dueCount} due · ${cardLabel}`
  }
  return cardLabel
}

export function SearchModal({ open, onClose }: SearchModalProps) {
  const storage = useStorage()
  const navigate = useNavigate()
  const location = useLocation()
  const dialogRef = useRef<HTMLDivElement | null>(null)
  const inputRef = useRef<HTMLInputElement | null>(null)
  const activeRowRef = useRef<HTMLDivElement | null>(null)
  const restoreFocusRef = useRef<HTMLElement | null>(null)
  const [query, setQuery] = useState('')
  const [settledQuery, setSettledQuery] = useState('')
  const [decks, setDecks] = useState<DeckTodaySummary[]>([])
  const [hits, setHits] = useState<CardSearchHit[]>([])
  const [loading, setLoading] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const [retrySeq, setRetrySeq] = useState(0)
  const [activeIndex, setActiveIndex] = useState(0)

  const trimmedQuery = query.trim()

  useEffect(() => {
    if (!open) {
      setQuery('')
      setSettledQuery('')
      setHits([])
      setLoading(false)
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

  // Load decks when the modal opens (and keep them fresh if storage updates while open).
  useEffect(() => {
    if (!open) return
    let cancelled = false
    let reqId = 0
    async function loadDecks() {
      const id = ++reqId
      try {
        const summary = await storage.getTodaySummary(Date.now())
        if (!cancelled && id === reqId) setDecks(summary.decks)
      } catch {
        try {
          const list = await storage.listDecks()
          if (!cancelled && id === reqId) {
            setDecks(
              list.map((d) => ({
                id: d.id,
                name: d.name,
                color: d.color,
                totalCards: 0,
                dueCount: 0,
                newDueCount: 0,
                masteredRatio: 0,
                desiredRetention: d.settings.desiredRetention,
              })),
            )
          }
        } catch {
          if (!cancelled && id === reqId) setDecks([])
        }
      }
    }
    void loadDecks()
    const unsubscribe = storage.subscribe(() => {
      void loadDecks()
    })
    return () => {
      cancelled = true
      unsubscribe()
    }
  }, [open, storage])

  // Intercept Escape in capture phase so underlying window listeners (e.g. CardEditorPage, Sheet)
  // do not fire when dismissing the search modal.
  useEffect(() => {
    if (!open) return
    function onWindowKey(e: KeyboardEvent) {
      if (e.isComposing || e.keyCode === 229) return
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        onClose()
      } else if (e.key === 'Tab' && dialogRef.current) {
        const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
          'input, button:not(:disabled), [href], [tabindex]:not([tabindex="-1"])',
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
    if (!trimmedQuery) {
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
        .searchCards(trimmedQuery, { limit: 40 })
        .then((results) => {
          if (cancelled) return
          setHits(results)
          setSettledQuery(trimmedQuery)
          setActiveIndex(0)
          setLoading(false)
        })
        .catch((err: unknown) => {
          if (cancelled) return
          console.error('card search failed', err)
          setHits([])
          setSettledQuery(trimmedQuery)
          setSearchError('Search failed. Please try again.')
          setActiveIndex(0)
          setLoading(false)
        })
    }, 60)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [open, trimmedQuery, retrySeq, storage])

  const highlightTerms = useMemo(() => extractHighlightTerms(trimmedQuery), [trimmedQuery])
  const highlightRegex = useMemo(() => buildHighlightRegex(highlightTerms), [highlightTerms])
  const deckHighlightRegex = useMemo(
    () => buildHighlightRegex(extractDeckHighlightTerms(trimmedQuery)),
    [trimmedQuery],
  )

  const visibleDecks = useMemo(() => {
    const sorted = [...decks].sort((a, b) => {
      if (a.dueCount !== b.dueCount) return b.dueCount - a.dueCount
      return a.name.localeCompare(b.name)
    })
    if (!trimmedQuery) {
      return sorted.slice(0, EMPTY_DECK_LIMIT)
    }
    return sorted
      .filter((deck) => matchesDeckName(deck.name, trimmedQuery))
      .slice(0, QUERY_DECK_LIMIT)
  }, [decks, trimmedQuery])

  const cardsCurrent = !loading && settledQuery === trimmedQuery && trimmedQuery.length > 0
  const visibleCards = trimmedQuery ? hits : []

  const options = useMemo<SearchOption[]>(() => {
    const list: SearchOption[] = visibleDecks.map((deck) => ({
      kind: 'deck',
      id: `search-opt-deck-${deck.id}`,
      deck,
    }))
    for (const hit of visibleCards) {
      list.push({
        kind: 'card',
        id: `search-opt-card-${hit.card.id}`,
        hit,
      })
    }
    return list
  }, [visibleDecks, visibleCards])

  const safeActiveIndex = options.length === 0 ? -1 : Math.min(activeIndex, options.length - 1)
  const activeOptionId = safeActiveIndex >= 0 ? options[safeActiveIndex].id : undefined

  useEffect(() => {
    activeRowRef.current?.scrollIntoView?.({ block: 'nearest' })
  }, [safeActiveIndex, options.length])

  if (!open) return null

  function openDeck(deck: DeckTodaySummary) {
    restoreFocusRef.current = null
    onClose()
    navigate(`/decks/${deck.id}`)
  }

  function openCardHit(hit: CardSearchHit) {
    if (!cardsCurrent) return
    restoreFocusRef.current = null
    onClose()
    navigate(`/decks/${hit.card.deckId}/cards/${hit.card.id}/edit`, {
      state: { returnTo: `${location.pathname}${location.search}` },
    })
  }

  function activateOption(option: SearchOption) {
    if (option.kind === 'deck') {
      openDeck(option.deck)
    } else {
      openCardHit(option.hit)
    }
  }

  function handleClear() {
    setQuery('')
    setActiveIndex(0)
    inputRef.current?.focus()
  }

  function handleRetry() {
    setRetrySeq((n) => n + 1)
    inputRef.current?.focus()
  }

  function handleInputKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.nativeEvent.isComposing) return
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setActiveIndex((idx) => {
        if (options.length === 0) return 0
        const cur = Math.min(idx, options.length - 1)
        return (cur + 1) % options.length
      })
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActiveIndex((idx) => {
        if (options.length === 0) return 0
        const cur = Math.min(idx, options.length - 1)
        return (cur - 1 + options.length) % options.length
      })
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      if (safeActiveIndex >= 0 && options[safeActiveIndex]) {
        activateOption(options[safeActiveIndex])
      }
    }
  }

  const deckCount = visibleDecks.length
  const cardCount = visibleCards.length
  const isSearchingCards = trimmedQuery.length > 0 && !cardsCurrent && !searchError

  const footerStatus = (() => {
    if (!trimmedQuery) {
      return decks.length > 0
        ? `${decks.length} ${decks.length === 1 ? 'deck' : 'decks'}`
        : ''
    }
    if (searchError) return ''
    if (isSearchingCards && options.length > 0) return 'Updating…'
    if (isSearchingCards) return ''
    const parts: string[] = []
    if (deckCount > 0) parts.push(`${deckCount} ${deckCount === 1 ? 'deck' : 'decks'}`)
    if (cardCount > 0) parts.push(`${cardCount} ${cardCount === 1 ? 'card' : 'cards'}`)
    if (parts.length === 0) return 'No results'
    return parts.join(' · ')
  })()

  const showEmptyNoQuery = !trimmedQuery && deckCount === 0
  const showEmptySearching = trimmedQuery.length > 0 && isSearchingCards && options.length === 0
  const showEmptyNoMatches =
    trimmedQuery.length > 0 && !searchError && !isSearchingCards && options.length === 0
  const showErrorOnly = Boolean(searchError) && deckCount === 0

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
        aria-label="Search decks and cards"
      >
        <div className="search-modal-input-row">
          <svg className="search-modal-icon" viewBox="0 0 16 16" aria-hidden="true">
            <circle cx="7" cy="7" r="4.5" />
            <path d="m10.5 10.5 3 3" />
          </svg>
          <input
            ref={inputRef}
            type="search"
            role="combobox"
            className="search-modal-input"
            placeholder="Search decks, cards, tags, and rationale…"
            aria-label="Search all cards"
            aria-expanded={options.length > 0}
            aria-autocomplete="list"
            aria-controls={options.length > 0 ? 'search-modal-listbox' : undefined}
            aria-activedescendant={activeOptionId}
            autoComplete="off"
            spellCheck={false}
            value={query}
            onKeyDown={handleInputKeyDown}
            onChange={(e) => {
              const next = e.target.value
              setQuery(next)
              if (next.trim() !== trimmedQuery) {
                setActiveIndex(0)
              }
            }}
          />
          {query.length > 0 && (
            <button
              type="button"
              className="search-modal-clear"
              aria-label="Clear search"
              onClick={handleClear}
            >
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <path d="M4.5 4.5 11.5 11.5M11.5 4.5 4.5 11.5" />
              </svg>
            </button>
          )}
          <button
            type="button"
            className="search-modal-esc"
            aria-label="Close search"
            onClick={onClose}
          >
            Esc
          </button>
        </div>

        <div className="search-modal-body">
          {showEmptyNoQuery ? (
            <p className="search-modal-empty">
              Type to search cards, tags, and draft rationale across your collection.
            </p>
          ) : showErrorOnly ? (
            <div className="search-modal-empty search-modal-error">
              <span role="alert">{searchError}</span>
              <button type="button" className="search-retry-btn" onClick={handleRetry}>
                Retry
              </button>
            </div>
          ) : showEmptySearching ? (
            <p className="search-modal-empty">Searching…</p>
          ) : showEmptyNoMatches ? (
            <p className="search-modal-empty">No results for “{trimmedQuery}”.</p>
          ) : (
            <>
              <div
                id="search-modal-listbox"
                className="search-modal-results"
                role="listbox"
                aria-label="Search results"
                aria-busy={isSearchingCards}
              >
                {deckCount > 0 && (
                  <div className="search-group" role="group" aria-labelledby="search-group-decks">
                    <div id="search-group-decks" className="search-section-label">
                      {trimmedQuery ? 'Decks' : 'Jump to deck'}
                    </div>
                    {visibleDecks.map((deck, idx) => {
                      const optionIndex = idx
                      const option = options[optionIndex]
                      const isSelected = optionIndex === safeActiveIndex
                      return (
                        <div
                          key={deck.id}
                          id={option.id}
                          ref={isSelected ? activeRowRef : null}
                          role="option"
                          tabIndex={-1}
                          aria-selected={isSelected}
                          className={`search-result-row search-deck-row${isSelected ? ' is-active' : ''}`}
                          onMouseDown={(e) => e.preventDefault()}
                          onMouseEnter={() => setActiveIndex(optionIndex)}
                          onClick={() => openDeck(deck)}
                        >
                          <span className="search-deck-main">
                            <span
                              className="search-result-dot"
                              style={{ backgroundColor: deck.color }}
                              aria-hidden="true"
                            />
                            <span className="search-deck-name">
                              {renderHighlighted(formatDeckPath(deck.name), deckHighlightRegex)}
                            </span>
                          </span>
                          <span className="search-deck-meta">{formatDeckMeta(deck)}</span>
                        </div>
                      )
                    })}
                  </div>
                )}

                {trimmedQuery.length > 0 && cardCount > 0 && (
                  <div className="search-group" role="group" aria-labelledby="search-group-cards">
                    <div id="search-group-cards" className="search-section-label">
                      Cards
                    </div>
                    {visibleCards.map((hit, idx) => {
                      const optionIndex = deckCount + idx
                      const option = options[optionIndex]
                      const isSelected = optionIndex === safeActiveIndex
                      const frontPlain = stripMarkdown(hit.card.front)
                      const backPlain = hit.card.back.trim() ? stripMarkdown(hit.card.back) : ''
                      const rationalePlain = hit.card.rationale?.trim()
                        ? stripMarkdown(hit.card.rationale)
                        : ''
                      const matchingTags = hit.card.tags.filter((t) =>
                        textMatchesQuery(t, highlightTerms),
                      )
                      const matchingTagSet = new Set(matchingTags)
                      const orderedTags = [
                        ...matchingTags,
                        ...hit.card.tags.filter((t) => !matchingTagSet.has(t)),
                      ].slice(0, 5)

                      const matchedPrimary =
                        textMatchesQuery(frontPlain, highlightTerms) ||
                        textMatchesQuery(hit.card.front, highlightTerms) ||
                        textMatchesQuery(backPlain, highlightTerms) ||
                        textMatchesQuery(hit.card.back, highlightTerms) ||
                        matchingTags.length > 0
                      const matchedRationaleOnly =
                        Boolean(rationalePlain) &&
                        !matchedPrimary &&
                        textMatchesQuery(rationalePlain, highlightTerms)

                      const frontText = frontPlain
                        ? windowedSnippet(frontPlain, highlightTerms, 120)
                        : ''
                      const backText = backPlain
                        ? windowedSnippet(backPlain, highlightTerms, 120)
                        : ''
                      const rationaleText =
                        matchedRationaleOnly && rationalePlain
                          ? windowedSnippet(rationalePlain, highlightTerms, 110)
                          : ''

                      return (
                        <div
                          key={hit.card.id}
                          id={option.id}
                          ref={isSelected ? activeRowRef : null}
                          role="option"
                          tabIndex={-1}
                          aria-selected={isSelected}
                          aria-disabled={isSearchingCards || undefined}
                          className={`search-result-row${isSelected ? ' is-active' : ''}${isSearchingCards ? ' is-stale' : ''}`}
                          onMouseDown={(e) => e.preventDefault()}
                          onMouseEnter={() => setActiveIndex(optionIndex)}
                          onClick={() => openCardHit(hit)}
                        >
                          <div className="search-result-top">
                            <span className="search-result-deck">
                              <span
                                className="search-result-dot"
                                style={{ backgroundColor: hit.deckColor }}
                                aria-hidden="true"
                              />
                              <span className="search-result-deck-name">
                                {formatDeckPath(hit.deckName)}
                              </span>
                            </span>
                            <span className="search-result-badges">
                              {matchedRationaleOnly && (
                                <span className="search-result-pill">Rationale</span>
                              )}
                              {hit.card.suspended && (
                                <span className="search-result-pill">Suspended</span>
                              )}
                            </span>
                          </div>
                          <div className="search-result-front">
                            {hit.card.clozeIndex != null && hit.card.clozeIndex > 0 && (
                              <span className="card-cloze-badge">
                                {hasImageOcclusion(hit.card.front)
                                  ? `IO ${hit.card.clozeIndex}`
                                  : `c${hit.card.clozeIndex}`}
                              </span>
                            )}
                            <span className="search-result-text">
                              {frontText
                                ? renderHighlighted(frontText, highlightRegex)
                                : '(empty front)'}
                            </span>
                          </div>
                          {backText && (
                            <div className="search-result-back">
                              {renderHighlighted(backText, highlightRegex)}
                            </div>
                          )}
                          {rationaleText && (
                            <div className="search-result-rationale">
                              {renderHighlighted(rationaleText, highlightRegex)}
                            </div>
                          )}
                          {orderedTags.length > 0 && (
                            <div className="search-result-tags">
                              {orderedTags.map((tag) => (
                                <span
                                  key={tag}
                                  className={`search-result-tag${matchingTagSet.has(tag) ? ' is-match' : ''}`}
                                >
                                  #{tag}
                                </span>
                              ))}
                            </div>
                          )}
                        </div>
                      )
                    })}
                  </div>
                )}
              </div>

              {trimmedQuery.length > 0 && cardCount === 0 && (isSearchingCards || searchError) && (
                <div className="search-cards-notice">
                  <div className="search-section-label">Cards</div>
                  {searchError ? (
                    <div className="search-inline-error">
                      <span role="alert">{searchError}</span>
                      <button type="button" className="search-retry-btn" onClick={handleRetry}>
                        Retry
                      </button>
                    </div>
                  ) : (
                    <p className="search-inline-status">Searching…</p>
                  )}
                </div>
              )}
            </>
          )}
        </div>

        <div className="search-modal-footer">
          <div className="search-footer-hints" aria-hidden="true">
            <span className="search-footer-hint">
              <kbd className="search-footer-kbd">↑↓</kbd>
              Navigate
            </span>
            <span className="search-footer-hint">
              <kbd className="search-footer-kbd">↵</kbd>
              Open
            </span>
            <span className="search-footer-hint">
              <kbd className="search-footer-kbd">Esc</kbd>
              Close
            </span>
          </div>
          <span className="search-footer-status" role="status" aria-live="polite">
            {footerStatus}
          </span>
        </div>
      </div>
    </div>
  )
}
