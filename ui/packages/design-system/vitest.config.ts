import { defineConfig } from "vitest/config";

const TEST_FILE_PATTERN = "src/**/*.test.{ts,tsx}" as const;

// Time tests assert the calendar of the UTC runners CI uses. Pinning the zone
// here, before any worker starts, grades them the same on a developer machine
// in IST or New York instead of failing on the machine's own clock.
process.env.TZ = "UTC";

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
        statements: 100,
        branches: 100,
        functions: 100,
        lines: 100,
      },
    },
  },
});
