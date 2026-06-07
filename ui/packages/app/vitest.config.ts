import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

const TEST_TIMEOUT_MS = 10_000

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
    testTimeout: TEST_TIMEOUT_MS,
    setupFiles: ['./vitest.setup.ts'],
    // Global DOM environment so every test file can render React components
    // and fire interactions through @testing-library. Node-only tests still
    // work under happy-dom (it provides a superset of the globals they use).
    environment: 'happy-dom',
    coverage: {
      // Istanbul (source-instrumented counters), not the default v8 provider.
      // v8 maps executed byte-ranges back onto the AST and merges across
      // workers; for async React components (ZombieThread's steer/retry paths)
      // that remap is non-deterministic under parallel load — branch coverage
      // flickered run-to-run at this suite's exact-97% margin (vitest#7660,
      // "v8 does not properly report react coverage, istanbul does"; #9725).
      // Istanbul increments a per-branch counter at execution, so the count is
      // deterministic. Other packages keep v8 (website has slack; design-system
      // can switch the same way if it ever flakes).
      provider: 'istanbul',
      reporter: ['text', 'lcov'],
      include: [
        'app/**/*.tsx',
        'app/**/*.ts',
        'components/analytics/**/*.tsx',
        'components/domain/**/*.tsx',
        'components/layout/**/*.tsx',
        'lib/**/*.ts',
        'instrumentation-client.ts',
      ],
      exclude: [
        'lib/types.ts',
        'next-env.d.ts',
        'next.config.ts',
        'playwright.config.ts',
        'global.d.ts',
        'proxy.ts',
        // Test infrastructure, not production code. The shared mock harnesses
        // (tests/helpers/*.tsx) build React elements via React.createElement,
        // so the `app/**/*.tsx` include glob otherwise sweeps them into the
        // production-coverage denominator. Coverage measures shipped code.
        'tests/**',
      ],
      thresholds: {
        statements: 100,
        branches: 100,
        functions: 100,
        lines: 100,
      },
    },
  },
})
