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
