import { useEffect, useState } from 'react'
import { useAssetState } from './assetUrl'
import { parseAssetSrc } from './imageSrc'
import type { OcclusionBlock, OcclusionMask } from './occlusion'

function MaskGeometry({
  mask,
  natW,
  natH,
  className,
}: {
  mask: OcclusionMask
  natW: number
  natH: number
  className: string
}) {
  const x = mask.x * natW
  const y = mask.y * natH
  const w = mask.w * natW
  const h = mask.h * natH
  if (mask.type === 'ellipse') {
    return (
      <ellipse
        cx={x + w / 2}
        cy={y + h / 2}
        rx={w / 2}
        ry={h / 2}
        className={className}
        vectorEffect="non-scaling-stroke"
      />
    )
  }
  const rx = Math.min(w, h) * 0.08
  return (
    <rect
      x={x}
      y={y}
      width={w}
      height={h}
      rx={Math.min(rx, 8)}
      ry={Math.min(rx, 8)}
      className={className}
      vectorEffect="non-scaling-stroke"
    />
  )
}

/**
 * Renders a Markdown-native Image Occlusion diagram with crisp SVG mask overlays.
 * - `activeIndex == null`: preview mode (shows all masks numbered).
 * - `activeIndex >= 1` & `!revealed`: hides active mask in accent color; hides inactive masks when `mode === 'hide-all'`.
 * - `activeIndex >= 1` & `revealed`: unveils active mask with subtle outline and offers a toggle to peek other masks.
 */
export function OcclusionView({
  block,
  activeIndex,
  revealed = false,
}: {
  block: OcclusionBlock
  activeIndex?: number | null
  revealed?: boolean
}) {
  const parsed = parseAssetSrc(block.image)
  const { asset, loading } = useAssetState(parsed?.hash)
  const [natSize, setNatSize] = useState<{ w: number; h: number }>({ w: 1000, h: 1000 })
  const [peekAll, setPeekAll] = useState(false)

  useEffect(() => {
    setPeekAll(false)
  }, [block.image, activeIndex, revealed])

  if (loading) {
    return <div className="io-placeholder">Loading diagram…</div>
  }
  if (!asset) {
    return (
      <div className="io-placeholder io-placeholder--missing">
        Diagram unavailable ({block.alt || block.image.slice(0, 18)})
      </div>
    )
  }

  const hasActive = typeof activeIndex === 'number' && activeIndex >= 1
  const hasInactiveMasks =
    hasActive && block.masks.some((m) => m.id !== activeIndex)
  const showPeekToggle = Boolean(
    hasActive && revealed && block.mode === 'hide-all' && hasInactiveMasks,
  )

  const badgeFontSize = Math.max(11, Math.min(natSize.w, natSize.h) * 0.028)

  return (
    <div className="io-view" data-occlusion-mode={block.mode}>
      <div className="io-stage">
        <img
          className="io-img"
          src={asset.url}
          alt={block.alt ?? 'Occluded diagram'}
          onLoad={(e) => {
            const img = e.currentTarget
            if (img.naturalWidth > 0 && img.naturalHeight > 0) {
              setNatSize({ w: img.naturalWidth, h: img.naturalHeight })
            }
          }}
        />
        <svg
          className="io-svg"
          viewBox={`0 0 ${natSize.w} ${natSize.h}`}
          preserveAspectRatio="none"
          aria-label={
            hasActive
              ? revealed
                ? `Image occlusion mask ${activeIndex} revealed`
                : `Image occlusion mask ${activeIndex} hidden`
              : `Image occlusion diagram with ${block.masks.length} masks`
          }
        >
          {block.masks.map((mask, idx) => {
            const isActive = hasActive && mask.id === activeIndex

            if (!hasActive) {
              return (
                <g key={`${mask.id}-${idx}`} data-mask-id={mask.id} data-mask-state="preview">
                  <MaskGeometry
                    mask={mask}
                    natW={natSize.w}
                    natH={natSize.h}
                    className="io-mask io-mask--preview"
                  />
                  <text
                    x={(mask.x + mask.w / 2) * natSize.w}
                    y={(mask.y + mask.h / 2) * natSize.h}
                    textAnchor="middle"
                    dominantBaseline="central"
                    fontSize={badgeFontSize}
                    className="io-mask-label"
                  >
                    {mask.id}
                  </text>
                </g>
              )
            }

            if (isActive) {
              return (
                <g
                  key={`${mask.id}-${idx}`}
                  data-mask-id={mask.id}
                  data-mask-state={revealed ? 'revealed' : 'active'}
                >
                  <MaskGeometry
                    mask={mask}
                    natW={natSize.w}
                    natH={natSize.h}
                    className={revealed ? 'io-mask io-mask--revealed' : 'io-mask io-mask--active'}
                  />
                  {!revealed && (
                    <text
                      x={(mask.x + mask.w / 2) * natSize.w}
                      y={(mask.y + mask.h / 2) * natSize.h}
                      textAnchor="middle"
                      dominantBaseline="central"
                      fontSize={badgeFontSize}
                      className="io-mask-qmark"
                    >
                      ?
                    </text>
                  )}
                </g>
              )
            }

            // Inactive mask
            if (block.mode === 'hide-one') {
              return null
            }

            return (
              <g
                key={`${mask.id}-${idx}`}
                data-mask-id={mask.id}
                data-mask-state={revealed && peekAll ? 'peek' : 'inactive'}
              >
                <MaskGeometry
                  mask={mask}
                  natW={natSize.w}
                  natH={natSize.h}
                  className={
                    revealed && peekAll ? 'io-mask io-mask--peek' : 'io-mask io-mask--inactive'
                  }
                />
              </g>
            )
          })}
        </svg>
      </div>
      {showPeekToggle && (
        <div className="io-view-controls">
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => setPeekAll((v) => !v)}
          >
            {peekAll ? 'Hide other masks' : 'Show all masks'}
          </button>
        </div>
      )}
    </div>
  )
}
