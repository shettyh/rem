import { useEffect, useId, useRef } from 'react'

export function ConfirmDialog({
  title,
  description,
  confirmLabel,
  cancelLabel = 'Cancel',
  onConfirm,
  onCancel,
  danger = false,
  confirmDisabled = false,
  cancelDisabled = false,
}: {
  title: string
  description: string
  confirmLabel: string
  cancelLabel?: string
  onConfirm: () => void
  onCancel: () => void
  danger?: boolean
  confirmDisabled?: boolean
  cancelDisabled?: boolean
}) {
  const dialogRef = useRef<HTMLDialogElement>(null)
  const unmountingRef = useRef(false)
  const titleId = useId()
  const descriptionId = useId()

  useEffect(() => {
    const dialog = dialogRef.current
    if (!dialog) return
    unmountingRef.current = false
    dialog.showModal()
    return () => {
      unmountingRef.current = true
      dialog.close()
    }
  }, [])

  return (
    <dialog
      ref={dialogRef}
      className="confirm-dialog"
      aria-labelledby={titleId}
      aria-describedby={descriptionId}
      onCancel={(event) => {
        event.preventDefault()
        if (!cancelDisabled) onCancel()
      }}
      onClose={() => {
        if (unmountingRef.current) return
        if (cancelDisabled) {
          dialogRef.current?.showModal()
        } else {
          onCancel()
        }
      }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <h2 id={titleId}>{title}</h2>
      <p id={descriptionId}>{description}</p>
      <div className="confirm-dialog-actions">
        <button
          type="button"
          className="btn btn-ghost"
          autoFocus
          disabled={cancelDisabled}
          onClick={onCancel}
        >
          {cancelLabel}
        </button>
        <button
          type="button"
          className={danger ? 'btn btn-danger' : 'btn btn-primary'}
          disabled={confirmDisabled}
          onClick={onConfirm}
        >
          {confirmLabel}
        </button>
      </div>
    </dialog>
  )
}
