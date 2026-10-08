import { defineConfig } from "vitest/config";

// The copied React tests use the source workspace's Vitest and jsdom semantics.
const TEST_PATTERN = "fixtures/p*/test.tsx";
const SETUP_FILE = "./helpers/setup.ts";
const TEST_ENVIRONMENT = "jsdom";
const TEST_TIMEOUT_MS = 10_000;

export default defineConfig({
  root: import.meta.dirname,
  oxc: { jsx: { runtime: "automatic" } },
  test: {
    environment: TEST_ENVIRONMENT,
    setupFiles: [SETUP_FILE],
    include: [TEST_PATTERN],
    testTimeout: TEST_TIMEOUT_MS,
    maxWorkers: 4,
  },
});
