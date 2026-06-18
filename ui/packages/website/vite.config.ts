import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const TEST_FILE_PATTERN = "src/**/*.test.{ts,tsx}" as const;

export default defineConfig({
  plugins: [tailwindcss(), react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: [TEST_FILE_PATTERN],
    exclude: ["tests/e2e/**", "node_modules/**", "dist/**"],
    css: true,
    maxWorkers: 2,
    minWorkers: 1,
    coverage: {
      provider: "v8",
      reporter: ["text", "json-summary", "lcov"],
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/main.tsx",
        "src/test-setup.ts",
        "src/vite-env.d.ts",
        TEST_FILE_PATTERN,
        // Visual DS gallery page. Zero business logic; rendering is
        // verified by tests/e2e/design-system-smoke.spec.ts.
        "src/pages/DesignSystemGallery.tsx",
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
