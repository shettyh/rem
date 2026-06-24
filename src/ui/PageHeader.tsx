import type { ReactNode } from 'react'

/**
 * The toolbar at the top of a screen: title on the left, context actions on the
 * right. The bar is a macOS window drag region (interactive children still
 * receive clicks); the traffic lights sit over the sidebar to its left.
 */
export function PageHeader({ title, actions }: { title: ReactNode; actions?: ReactNode }) {
  return (
    <header className="page-header" data-tauri-drag-region>
      <h1>{title}</h1>
      {actions && <div className="page-header-actions">{actions}</div>}
    </header>
  )
}
