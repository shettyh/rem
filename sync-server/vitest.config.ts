import { cloudflareTest } from '@cloudflare/vitest-pool-workers'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: './wrangler.toml' },
      miniflare: {
        bindings: {
          DEV_AUTH_TOKEN: 'test-dev-secret',
          JWT_SECRET: 'test-jwt-secret-key',
          MAX_ASSET_BYTES: '1024',
          MAX_USER_QUOTA_BYTES: '2048',
        },
      },
    }),
  ],
})
