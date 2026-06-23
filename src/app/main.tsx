import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import { router } from './routes'
import { StorageProvider, defaultStorage } from '../data/StorageContext'
import { useAutoSync } from './useAutoSync'
import '../ui/styles.css'

function App() {
  useAutoSync(defaultStorage)
  return <RouterProvider router={router} />
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <StorageProvider>
      <App />
    </StorageProvider>
  </StrictMode>,
)
