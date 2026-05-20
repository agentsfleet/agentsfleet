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
    // Vitest's default exclude is `['**/node_modules/**', '**/dist/**', ...]`.
    // Overriding `exclude` REPLACES the default — losing the node_modules
    // skip — so the workspace symlink `node_modules/@usezombie/design-system`
    // got followed into the design-system's own tests, which ran without
    // that package's `test-setup.ts` and failed on `toBeInTheDocument`.
    // Restoring the defaults alongside the e2e exclude fixes the flake.
    exclude: [
      '**/node_modules/**',
      '**/dist/**',
      '**/.{idea,git,cache,output,temp}/**',
      'tests/e2e/**',
    ],
    testTimeout: 10000,
    setupFiles: ['./vitest.setup.ts'],
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
