import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  cacheDir: '.tmp/vite',
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./', import.meta.url)),
    },
  },
  test: {
    exclude: ['tests/e2e/**'],
    testTimeout: 10000,
    // Global DOM environment so every test file can render React components
    // and fire interactions through @testing-library. Node-only tests still
    // work under happy-dom (it provides a superset of the globals they use).
    environment: 'happy-dom',
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      include: [
        'app/**/*.tsx',
        'components/analytics/**/*.tsx',
        'components/domain/**/*.tsx',
        'components/layout/**/*.tsx',
        'lib/**/*.ts',
        'instrumentation-client.ts',
      ],
      exclude: [
        'lib/types.ts',
        'components/ui/**',
        'next-env.d.ts',
        'next.config.ts',
        'playwright.config.ts',
        'global.d.ts',
        'proxy.ts',
        // Dev-only fixture route. ds-button-rsc is a build-time assertion —
        // its contract is that `next build` does not hoist "use client"
        // onto the DS Button — not a runtime unit.
        '**/ds-button-rsc/**',
      ],
      thresholds: {
        statements: 95,
        branches: 90,
        functions: 95,
        lines: 95,
      },
    },
  },
})
