import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import { router } from './routes'
import { StorageProvider } from '../data/StorageContext'
import '../ui/styles.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <StorageProvider>
      <RouterProvider router={router} />
    </StorageProvider>
  </StrictMode>,
)
