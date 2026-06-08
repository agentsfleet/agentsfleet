import { defineConfig } from "vitest/config";

const TEST_FILE_PATTERN = "src/**/*.test.{ts,tsx}" as const;

export default defineConfig({
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: [TEST_FILE_PATTERN],
    coverage: {
      provider: "v8",
      reporter: ["text", "json-summary", "lcov"],
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        TEST_FILE_PATTERN,
        "src/test-setup.ts",
        // Pure re-export barrels — no logic to cover.
        "src/index.ts",
        "src/design-system/index.ts",
      ],
      thresholds: {
        statements: 99,
        branches: 99,
        functions: 99,
        lines: 99,
      },
    },
  },
});
