/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { playwright } from '@vitest/browser-playwright'

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'unit',
          globals: true,
          environment: 'jsdom',
          setupFiles: './src/test/setup.ts',
          include: ['src/**/*.test.{ts,tsx}'],
          exclude: ['src/**/*.browser.test.tsx', 'node_modules/**'],
        },
      },
      {
        extends: true,
        test: {
          name: 'browser',
          globals: true,
          setupFiles: './src/test/browser-setup.ts',
          include: ['src/**/*.browser.test.tsx'],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            instances: [{ browser: 'chromium', viewport: { width: 1280, height: 800 } }],
          },
        },
      },
    ],
  },
})
