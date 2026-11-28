import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react'
import { useAssetState } from './assetUrl'
import { parseAssetSrc } from './imageSrc'
import {
  MAX_OCCLUSION_MASKS,
  nextOcclusionMaskId,
  type OcclusionBlock,
  type OcclusionMask,
  type OcclusionMode,
  type OcclusionShapeType,
} from './occlusion'

type EditorTool = 'rect' | 'ellipse' | 'select' | 'pan'
type ResizeHandle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w'

interface DraftBox {
  type: OcclusionShapeType
  startX: number
  startY: number
  currX: number
  currY: number
}

type InteractionState =
  | { kind: 'idle' }
  | { kind: 'draw'; draft: DraftBox }
  | {
      kind: 'move'
      startX: number
      startY: number
      indices: number[]
      origins: { x: number; y: number; w: number; h: number }[]
      snapshot: OcclusionMask[]
      moved: boolean
    }
  | {
      kind: 'resize'
      index: number
      handle: ResizeHandle
      origin: { x: number; y: number; w: number; h: number }
      snapshot: OcclusionMask[]
      moved: boolean
    }
  | {
      kind: 'pan'
      startClientX: number
      startClientY: number
      originPanX: number
      originPanY: number
    }

const MIN_DRAW_EXTENT = 0.012
const MIN_RESIZE_EXTENT = 0.015
const MAX_HISTORY = 50
const HANDLES: { id: ResizeHandle; cx: (m: OcclusionMask) => number; cy: (m: OcclusionMask) => number; cursor: string }[] = [
  { id: 'nw', cx: (m) => m.x, cy: (m) => m.y, cursor: 'nwse-resize' },
  { id: 'n', cx: (m) => m.x + m.w / 2, cy: (m) => m.y, cursor: 'ns-resize' },
  { id: 'ne', cx: (m) => m.x + m.w, cy: (m) => m.y, cursor: 'nesw-resize' },
  { id: 'e', cx: (m) => m.x + m.w, cy: (m) => m.y + m.h / 2, cursor: 'ew-resize' },
  { id: 'se', cx: (m) => m.x + m.w, cy: (m) => m.y + m.h, cursor: 'nwse-resize' },
  { id: 's', cx: (m) => m.x + m.w / 2, cy: (m) => m.y + m.h, cursor: 'ns-resize' },
  { id: 'sw', cx: (m) => m.x, cy: (m) => m.y + m.h, cursor: 'nesw-resize' },
  { id: 'w', cx: (m) => m.x, cy: (m) => m.y + m.h / 2, cursor: 'ew-resize' },
]

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v))
}

function round4(v: number): number {
  return Math.round(v * 10_000) / 10_000
}

function trySetPointerCapture(el: Element | null | undefined, pointerId: number) {
  try {
    el?.setPointerCapture(pointerId)
  } catch {
    // Synthetic or already-released pointer IDs can reject setPointerCapture; drag still works.
  }
}

function normalizeBox(draft: DraftBox): { x: number; y: number; w: number; h: number } {
  const x = clamp(Math.min(draft.startX, draft.currX), 0, 1)
  const y = clamp(Math.min(draft.startY, draft.currY), 0, 1)
  const x2 = clamp(Math.max(draft.startX, draft.currX), 0, 1)
  const y2 = clamp(Math.max(draft.startY, draft.currY), 0, 1)
  return {
    x: round4(x),
    y: round4(y),
    w: round4(x2 - x),
    h: round4(y2 - y),
  }
}

/**
 * Interactive custom-SVG Image Occlusion editor embedded inside `CardEditorPage`.
 * Supports rectangle & ellipse masks, selection/multi-select, drag-move, 8-handle resize,
 * grouping multiple masks into one card, pan/zoom, keyboard nudging, and local undo/redo.
 */
export function OcclusionEditor({
  header,
  onHeaderChange,
  block,
  onBlockChange,
  onReplaceImage,
  onRemoveOcclusion,
}: {
  header: string
  onHeaderChange: (value: string) => void
  block: OcclusionBlock
  onBlockChange: (next: OcclusionBlock) => void
  onReplaceImage: (file: File) => Promise<void>
  onRemoveOcclusion?: () => void
}) {
  const parsed = parseAssetSrc(block.image)
  const { asset, loading } = useAssetState(parsed?.hash)
  const [natSize, setNatSize] = useState<{ w: number; h: number }>({ w: 1000, h: 1000 })
  const [tool, setTool] = useState<EditorTool>('rect')
  const [selected, setSelected] = useState<number[]>([])
  const [zoom, setZoom] = useState(1)
  const [pan, setPan] = useState<{ x: number; y: number }>({ x: 0, y: 0 })
  const [interaction, setInteraction] = useState<InteractionState>({ kind: 'idle' })
  const [undoStack, setUndoStack] = useState<OcclusionMask[][]>([])
  const [redoStack, setRedoStack] = useState<OcclusionMask[][]>([])

  const stageRef = useRef<HTMLDivElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const blockRef = useRef(block)
  blockRef.current = block

  // Prune selection indices if masks shrink externally
  useEffect(() => {
    setSelected((prev) => prev.filter((idx) => idx >= 0 && idx < block.masks.length))
  }, [block.masks.length])

  const commitMasks = useCallback(
    (nextMasks: OcclusionMask[], snapshotForUndo?: OcclusionMask[]) => {
      const base = snapshotForUndo ?? blockRef.current.masks
      setUndoStack((prev) => [...prev.slice(-(MAX_HISTORY - 1)), base])
      setRedoStack([])
      onBlockChange({ ...blockRef.current, masks: nextMasks })
    },
    [onBlockChange],
  )

  const undo = useCallback(() => {
    setUndoStack((prev) => {
      if (prev.length === 0) return prev
      const previous = prev[prev.length - 1]
      setRedoStack((r) => [...r.slice(-(MAX_HISTORY - 1)), blockRef.current.masks])
      onBlockChange({ ...blockRef.current, masks: previous })
      setSelected([])
      return prev.slice(0, -1)
    })
  }, [onBlockChange])

  const redo = useCallback(() => {
    setRedoStack((prev) => {
      if (prev.length === 0) return prev
      const next = prev[prev.length - 1]
      setUndoStack((u) => [...u.slice(-(MAX_HISTORY - 1)), blockRef.current.masks])
      onBlockChange({ ...blockRef.current, masks: next })
      setSelected([])
      return prev.slice(0, -1)
    })
  }, [onBlockChange])

  const getNormalizedPoint = useCallback((clientX: number, clientY: number) => {
    const el = stageRef.current
    if (!el) return null
    const rect = el.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0) return null
    return {
      x: clamp((clientX - rect.left) / rect.width, 0, 1),
      y: clamp((clientY - rect.top) / rect.height, 0, 1),
    }
  }, [])

  const deleteSelected = useCallback(() => {
    if (selected.length === 0) return
    const drop = new Set(selected)
    const next = blockRef.current.masks.filter((_, idx) => !drop.has(idx))
    commitMasks(next)
    setSelected([])
  }, [selected, commitMasks])

  const groupSelected = useCallback(() => {
    if (selected.length < 2) return
    const selSet = new Set(selected)
    const targetId = Math.min(...selected.map((idx) => blockRef.current.masks[idx]?.id ?? 1))
    const next = blockRef.current.masks.map((mask, idx) =>
      selSet.has(idx) ? { ...mask, id: targetId } : mask,
    )
    commitMasks(next)
  }, [selected, commitMasks])

  const ungroupSelected = useCallback(() => {
    if (selected.length === 0) return
    const next = [...blockRef.current.masks]
    let nextId = nextOcclusionMaskId(next)
    let changed = false
    for (const idx of selected) {
      const target = next[idx]
      if (!target) continue
      const peers = next.filter((m) => m.id === target.id)
      if (peers.length > 1) {
        next[idx] = { ...target, id: nextId }
        nextId += 1
        changed = true
      }
    }
    if (changed) commitMasks(next)
  }, [selected, commitMasks])

  const canUngroup = useMemo(() => {
    if (selected.length === 0) return false
    const counts = new Map<number, number>()
    for (const m of block.masks) {
      counts.set(m.id, (counts.get(m.id) ?? 0) + 1)
    }
    return selected.some((idx) => {
      const m = block.masks[idx]
      return m ? (counts.get(m.id) ?? 0) > 1 : false
    })
  }, [block.masks, selected])

  const cardGroupIds = useMemo(() => {
    const ids = new Set<number>()
    for (const m of block.masks) ids.add(m.id)
    return [...ids].sort((a, b) => a - b)
  }, [block.masks])

  const setMode = (mode: OcclusionMode) => {
    onBlockChange({ ...block, mode })
  }

  const onCanvasPointerDown = (e: ReactPointerEvent<SVGSVGElement>) => {
    if (e.button !== 0) return
    if (tool === 'pan') {
      trySetPointerCapture(e.currentTarget, e.pointerId)
      setInteraction({
        kind: 'pan',
        startClientX: e.clientX,
        startClientY: e.clientY,
        originPanX: pan.x,
        originPanY: pan.y,
      })
      return
    }
    if (tool === 'select') {
      setSelected([])
      return
    }
    if (block.masks.length >= MAX_OCCLUSION_MASKS) return
    const pt = getNormalizedPoint(e.clientX, e.clientY)
    if (!pt) return
    trySetPointerCapture(e.currentTarget, e.pointerId)
    setSelected([])
    setInteraction({
      kind: 'draw',
      draft: {
        type: tool,
        startX: pt.x,
        startY: pt.y,
        currX: pt.x,
        currY: pt.y,
      },
    })
  }

  const onMaskPointerDown = (e: ReactPointerEvent, index: number) => {
    if (e.button !== 0 || tool === 'pan') return
    if (tool === 'rect' || tool === 'ellipse') {
      // Allow drawing overlapping masks freely without selecting existing ones unless Shift is held
      return
    }
    e.stopPropagation()
    const pt = getNormalizedPoint(e.clientX, e.clientY)
    if (!pt) return

    let nextSelected: number[]
    if (e.shiftKey) {
      nextSelected = selected.includes(index)
        ? selected.filter((i) => i !== index)
        : [...selected, index]
    } else if (selected.includes(index)) {
      nextSelected = selected
    } else {
      nextSelected = [index]
    }
    setSelected(nextSelected)

    if (nextSelected.length === 0) return
    const svg = stageRef.current?.querySelector('svg')
    trySetPointerCapture(svg, e.pointerId)
    setInteraction({
      kind: 'move',
      startX: pt.x,
      startY: pt.y,
      indices: nextSelected,
      origins: nextSelected.map((i) => {
        const m = block.masks[i]
        return { x: m.x, y: m.y, w: m.w, h: m.h }
      }),
      snapshot: block.masks,
      moved: false,
    })
  }

  const onHandlePointerDown = (
    e: ReactPointerEvent,
    index: number,
    handle: ResizeHandle,
  ) => {
    if (e.button !== 0) return
    e.stopPropagation()
    const m = block.masks[index]
    if (!m) return
    const svg = stageRef.current?.querySelector('svg')
    trySetPointerCapture(svg, e.pointerId)
    setInteraction({
      kind: 'resize',
      index,
      handle,
      origin: { x: m.x, y: m.y, w: m.w, h: m.h },
      snapshot: block.masks,
      moved: false,
    })
  }

  const onCanvasPointerMove = (e: ReactPointerEvent<SVGSVGElement>) => {
    if (interaction.kind === 'idle') return
    if (interaction.kind === 'pan') {
      setPan({
        x: interaction.originPanX + (e.clientX - interaction.startClientX),
        y: interaction.originPanY + (e.clientY - interaction.startClientY),
      })
      return
    }
    const pt = getNormalizedPoint(e.clientX, e.clientY)
    if (!pt) return

    if (interaction.kind === 'draw') {
      setInteraction({
        kind: 'draw',
        draft: { ...interaction.draft, currX: pt.x, currY: pt.y },
      })
      return
    }

    if (interaction.kind === 'move') {
      const dx = pt.x - interaction.startX
      const dy = pt.y - interaction.startY
      if (!interaction.moved && Math.abs(dx) < 0.002 && Math.abs(dy) < 0.002) return
      const next = [...blockRef.current.masks]
      interaction.indices.forEach((maskIdx, pos) => {
        const orig = interaction.origins[pos]
        const existing = next[maskIdx]
        if (!orig || !existing) return
        const nx = round4(clamp(orig.x + dx, 0, 1 - orig.w))
        const ny = round4(clamp(orig.y + dy, 0, 1 - orig.h))
        next[maskIdx] = { ...existing, x: nx, y: ny }
      })
      if (!interaction.moved) {
        setInteraction({ ...interaction, moved: true })
      }
      onBlockChange({ ...blockRef.current, masks: next })
      return
    }

    if (interaction.kind === 'resize') {
      const { origin, handle, index } = interaction
      let left = origin.x
      let right = origin.x + origin.w
      let top = origin.y
      let bottom = origin.y + origin.h

      if (handle.includes('w')) left = clamp(pt.x, 0, right - MIN_RESIZE_EXTENT)
      if (handle.includes('e')) right = clamp(pt.x, left + MIN_RESIZE_EXTENT, 1)
      if (handle.includes('n')) top = clamp(pt.y, 0, bottom - MIN_RESIZE_EXTENT)
      if (handle.includes('s')) bottom = clamp(pt.y, top + MIN_RESIZE_EXTENT, 1)

      const next = [...blockRef.current.masks]
      const existing = next[index]
      if (!existing) return
      next[index] = {
        ...existing,
        x: round4(left),
        y: round4(top),
        w: round4(right - left),
        h: round4(bottom - top),
      }
      if (!interaction.moved) {
        setInteraction({ ...interaction, moved: true })
      }
      onBlockChange({ ...blockRef.current, masks: next })
    }
  }

  const onCanvasPointerUp = () => {
    if (interaction.kind === 'idle') return
    if (interaction.kind === 'draw') {
      const box = normalizeBox(interaction.draft)
      if (box.w >= MIN_DRAW_EXTENT && box.h >= MIN_DRAW_EXTENT) {
        const nextMask: OcclusionMask = {
          id: nextOcclusionMaskId(blockRef.current.masks),
          type: interaction.draft.type,
          ...box,
        }
        const nextMasks = [...blockRef.current.masks, nextMask]
        commitMasks(nextMasks)
        setSelected([nextMasks.length - 1])
      }
    } else if (
      (interaction.kind === 'move' || interaction.kind === 'resize') &&
      interaction.moved
    ) {
      setUndoStack((prev) => [...prev.slice(-(MAX_HISTORY - 1)), interaction.snapshot])
      setRedoStack([])
    }
    setInteraction({ kind: 'idle' })
  }

  const onCanvasKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const tag = (e.target as HTMLElement | null)?.tagName ?? ''
    if (tag === 'INPUT' || tag === 'TEXTAREA') return

    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault()
      e.stopPropagation()
      if (e.shiftKey) redo()
      else undo()
      return
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'y') {
      e.preventDefault()
      e.stopPropagation()
      redo()
      return
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return

    const lower = e.key.toLowerCase()
    if (lower === 'r') {
      e.preventDefault()
      setTool('rect')
    } else if (lower === 'e') {
      e.preventDefault()
      setTool('ellipse')
    } else if (lower === 'v') {
      e.preventDefault()
      setTool('select')
    } else if (lower === 'h') {
      e.preventDefault()
      setTool('pan')
    } else if ((e.key === 'Delete' || e.key === 'Backspace') && selected.length > 0) {
      e.preventDefault()
      e.stopPropagation()
      deleteSelected()
    } else if (
      ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key) &&
      selected.length > 0
    ) {
      e.preventDefault()
      e.stopPropagation()
      const step = e.shiftKey ? 0.0025 : 0.01
      const dx = e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0
      const dy = e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0
      const selSet = new Set(selected)
      const next = blockRef.current.masks.map((m, idx) =>
        selSet.has(idx)
          ? {
              ...m,
              x: round4(clamp(m.x + dx, 0, 1 - m.w)),
              y: round4(clamp(m.y + dy, 0, 1 - m.h)),
            }
          : m,
      )
      commitMasks(next)
    }
  }

  const singleSelectedIdx = selected.length === 1 ? selected[0] : null
  const singleSelectedMask =
    singleSelectedIdx !== null ? block.masks[singleSelectedIdx] ?? null : null
  const draftBox = interaction.kind === 'draw' ? normalizeBox(interaction.draft) : null
  const badgeFontSize = Math.max(12, Math.min(natSize.w, natSize.h) * 0.026)
  const handleRadius = Math.max(5, Math.min(natSize.w, natSize.h) * 0.011) / zoom

  return (
    <div
      className="io-editor"
      onKeyDown={onCanvasKeyDown}
      tabIndex={0}
      aria-label="Image Occlusion editor"
    >
      <div className="io-editor-header-row">
        <input
          type="text"
          className="text-input io-header-input"
          value={header}
          onChange={(e) => onHeaderChange(e.target.value)}
          placeholder="Optional title or prompt above diagram (e.g. TCP Header)…"
          aria-label="Diagram title"
        />
      </div>

      <div className="io-editor-toolbar" role="toolbar" aria-label="Image Occlusion tools">
        <div className="io-tool-group" role="group" aria-label="Drawing tools">
          <button
            type="button"
            className={`io-tool-btn${tool === 'rect' ? ' is-active' : ''}`}
            aria-pressed={tool === 'rect'}
            title="Rectangle mask (R)"
            onClick={() => setTool('rect')}
          >
            Rect
          </button>
          <button
            type="button"
            className={`io-tool-btn${tool === 'ellipse' ? ' is-active' : ''}`}
            aria-pressed={tool === 'ellipse'}
            title="Ellipse mask (E)"
            onClick={() => setTool('ellipse')}
          >
            Ellipse
          </button>
          <button
            type="button"
            className={`io-tool-btn${tool === 'select' ? ' is-active' : ''}`}
            aria-pressed={tool === 'select'}
            title="Select, move & resize masks (V)"
            onClick={() => setTool('select')}
          >
            Select
          </button>
          <button
            type="button"
            className={`io-tool-btn${tool === 'pan' ? ' is-active' : ''}`}
            aria-pressed={tool === 'pan'}
            title="Pan canvas (H)"
            onClick={() => setTool('pan')}
          >
            Pan
          </button>
        </div>

        <span className="md-tool-sep" />

        <div className="io-tool-group" role="group" aria-label="Mask actions">
          <button
            type="button"
            className="io-tool-btn"
            disabled={selected.length < 2}
            title="Group selected masks into one card"
            onClick={groupSelected}
          >
            Group
          </button>
          <button
            type="button"
            className="io-tool-btn"
            disabled={!canUngroup}
            title="Split selected masks into separate cards"
            onClick={ungroupSelected}
          >
            Ungroup
          </button>
          <button
            type="button"
            className="io-tool-btn"
            disabled={selected.length === 0}
            title="Delete selected mask (Delete)"
            onClick={deleteSelected}
          >
            Delete
          </button>
        </div>

        <span className="md-tool-sep" />

        <div className="io-tool-group" role="group" aria-label="Undo and redo">
          <button
            type="button"
            className="io-tool-btn"
            disabled={undoStack.length === 0}
            title="Undo mask edit (⌘Z)"
            onClick={undo}
          >
            Undo
          </button>
          <button
            type="button"
            className="io-tool-btn"
            disabled={redoStack.length === 0}
            title="Redo mask edit (⌘⇧Z)"
            onClick={redo}
          >
            Redo
          </button>
        </div>

        <span className="md-tool-sep" />

        <div className="io-tool-group" role="group" aria-label="Zoom">
          <button
            type="button"
            className="io-tool-btn"
            aria-label="Zoom out"
            onClick={() => setZoom((z) => clamp(Math.round((z - 0.25) * 100) / 100, 0.5, 3))}
          >
            −
          </button>
          <button
            type="button"
            className="io-tool-btn io-zoom-readout"
            title="Reset zoom and pan"
            onClick={() => {
              setZoom(1)
              setPan({ x: 0, y: 0 })
            }}
          >
            {Math.round(zoom * 100)}%
          </button>
          <button
            type="button"
            className="io-tool-btn"
            aria-label="Zoom in"
            onClick={() => setZoom((z) => clamp(Math.round((z + 0.25) * 100) / 100, 0.5, 3))}
          >
            +
          </button>
        </div>

        <div className="io-tool-spacer" />

        <div className="io-tool-group" role="group" aria-label="Occlusion mode">
          <button
            type="button"
            className={`io-tool-btn${block.mode === 'hide-all' ? ' is-active' : ''}`}
            aria-pressed={block.mode === 'hide-all'}
            onClick={() => setMode('hide-all')}
          >
            Hide all, guess one
          </button>
          <button
            type="button"
            className={`io-tool-btn${block.mode === 'hide-one' ? ' is-active' : ''}`}
            aria-pressed={block.mode === 'hide-one'}
            onClick={() => setMode('hide-one')}
          >
            Hide one, guess one
          </button>
        </div>
      </div>

      <div
        className={`io-canvas-viewport io-tool--${tool}`}
        onWheel={(e) => {
          if (e.ctrlKey || e.metaKey) {
            e.preventDefault()
            const delta = e.deltaY < 0 ? 0.15 : -0.15
            setZoom((z) => clamp(Math.round((z + delta) * 100) / 100, 0.5, 3))
          }
        }}
      >
        {loading ? (
          <div className="io-placeholder">Loading diagram…</div>
        ) : !asset ? (
          <div className="io-placeholder io-placeholder--missing">
            Image asset not found. Choose a replacement image below.
          </div>
        ) : (
          <div
            ref={stageRef}
            className="io-stage io-stage--editor"
            style={{
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
              transformOrigin: 'center center',
            }}
          >
            <img
              className="io-img"
              src={asset.url}
              alt={block.alt ?? 'Occlusion canvas'}
              draggable={false}
              onLoad={(e) => {
                const img = e.currentTarget
                if (img.naturalWidth > 0 && img.naturalHeight > 0) {
                  setNatSize({ w: img.naturalWidth, h: img.naturalHeight })
                }
              }}
            />
            <svg
              className="io-svg io-svg--interactive"
              viewBox={`0 0 ${natSize.w} ${natSize.h}`}
              preserveAspectRatio="none"
              onPointerDown={onCanvasPointerDown}
              onPointerMove={onCanvasPointerMove}
              onPointerUp={onCanvasPointerUp}
              onPointerCancel={onCanvasPointerUp}
            >
              {block.masks.map((mask, idx) => {
                const isSelected = selected.includes(idx)
                const x = mask.x * natSize.w
                const y = mask.y * natSize.h
                const w = mask.w * natSize.w
                const h = mask.h * natSize.h
                return (
                  <g
                    key={`mask-${idx}`}
                    data-editor-mask-index={idx}
                    data-mask-id={mask.id}
                    className={`io-edit-mask-group${isSelected ? ' is-selected' : ''}`}
                    onPointerDown={(e) => onMaskPointerDown(e, idx)}
                  >
                    {mask.type === 'ellipse' ? (
                      <ellipse
                        cx={x + w / 2}
                        cy={y + h / 2}
                        rx={w / 2}
                        ry={h / 2}
                        className={`io-mask io-mask--edit${isSelected ? ' is-selected' : ''}`}
                        vectorEffect="non-scaling-stroke"
                      />
                    ) : (
                      <rect
                        x={x}
                        y={y}
                        width={w}
                        height={h}
                        rx={Math.min(Math.min(w, h) * 0.08, 8)}
                        ry={Math.min(Math.min(w, h) * 0.08, 8)}
                        className={`io-mask io-mask--edit${isSelected ? ' is-selected' : ''}`}
                        vectorEffect="non-scaling-stroke"
                      />
                    )}
                    <text
                      x={x + w / 2}
                      y={y + h / 2}
                      textAnchor="middle"
                      dominantBaseline="central"
                      fontSize={badgeFontSize}
                      className="io-mask-label"
                    >
                      {mask.id}
                    </text>
                  </g>
                )
              })}

              {draftBox && interaction.kind === 'draw' && (
                interaction.draft.type === 'ellipse' ? (
                  <ellipse
                    cx={(draftBox.x + draftBox.w / 2) * natSize.w}
                    cy={(draftBox.y + draftBox.h / 2) * natSize.h}
                    rx={(draftBox.w / 2) * natSize.w}
                    ry={(draftBox.h / 2) * natSize.h}
                    className="io-mask io-mask--draft"
                    vectorEffect="non-scaling-stroke"
                  />
                ) : (
                  <rect
                    x={draftBox.x * natSize.w}
                    y={draftBox.y * natSize.h}
                    width={draftBox.w * natSize.w}
                    height={draftBox.h * natSize.h}
                    className="io-mask io-mask--draft"
                    vectorEffect="non-scaling-stroke"
                  />
                )
              )}

              {tool === 'select' &&
                singleSelectedIdx !== null &&
                singleSelectedMask &&
                HANDLES.map((h) => (
                  <circle
                    key={h.id}
                    cx={h.cx(singleSelectedMask) * natSize.w}
                    cy={h.cy(singleSelectedMask) * natSize.h}
                    r={handleRadius}
                    className="io-resize-handle"
                    style={{ cursor: h.cursor }}
                    vectorEffect="non-scaling-stroke"
                    data-resize-handle={h.id}
                    onPointerDown={(e) => onHandlePointerDown(e, singleSelectedIdx, h.id)}
                  />
                ))}
            </svg>
          </div>
        )}
      </div>

      <div className="io-editor-foot">
        <span className="io-editor-status">
          {block.masks.length === 0
            ? 'Drag on the image to draw your first mask (R rect · E ellipse · V select · H pan).'
            : `${block.masks.length} ${block.masks.length === 1 ? 'mask' : 'masks'} · ${cardGroupIds.length} ${cardGroupIds.length === 1 ? 'card' : 'cards'} (${cardGroupIds.map((id) => `#${id}`).join(', ')}) · Shift-click to multi-select & group`}
        </span>
        <div className="io-editor-foot-actions">
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            style={{ display: 'none' }}
            onChange={(e) => {
              const file = e.target.files?.[0]
              if (file) void onReplaceImage(file)
              e.target.value = ''
            }}
          />
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => fileInputRef.current?.click()}
          >
            Replace image
          </button>
          {onRemoveOcclusion && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={onRemoveOcclusion}
            >
              Switch to text card
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
