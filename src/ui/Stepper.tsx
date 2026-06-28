/** Numeric stepper: − value +, with optional clamp and display formatter. */
export function Stepper({
  value,
  onChange,
  label,
  step = 1,
  min,
  max,
  format,
}: {
  value: number
  onChange: (next: number) => void
  label: string
  step?: number
  min?: number
  max?: number
  format?: (v: number) => string
}) {
  const clamp = (v: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v))
  return (
    <div className="stepper">
      <button type="button" className="stepper-btn" aria-label={`Decrease ${label}`} onClick={() => onChange(clamp(value - step))}>
        −
      </button>
      <span className="stepper-val">{format ? format(value) : value}</span>
      <button type="button" className="stepper-btn" aria-label={`Increase ${label}`} onClick={() => onChange(clamp(value + step))}>
        +
      </button>
    </div>
  )
}
