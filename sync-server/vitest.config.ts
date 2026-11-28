import { cloudflareTest } from '@cloudflare/vitest-plugin'
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
          GOOGLE_CLIENT_ID: 'google-test-client-id',
          GOOGLE_CLIENT_SECRET: 'google-test-client-secret',
          APPLE_CLIENT_ID: 'com.shettyh.rem.test',
          APPLE_TEAM_ID: 'TEAM123456',
          APPLE_KEY_ID: 'KEY1234567',
        },
      },
    }),
  ],
})
