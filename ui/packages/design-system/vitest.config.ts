import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    coverage: {
      provider: "v8",
      reporter: ["text", "json-summary", "lcov"],
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/**/*.test.{ts,tsx}",
        "src/test-setup.ts",
        // Pure re-export barrels — no logic to cover.
        "src/index.ts",
        "src/design-system/index.ts",
      ],
      thresholds: {
        statements: 97,
        branches: 97,
        functions: 97,
        lines: 97,
      },
    },
  },
});
