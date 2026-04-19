import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [tailwindcss(), react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
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
        "src/**/*.test.{ts,tsx}",
        // Canvas + rAF-heavy animation components. Static render + prop
        // surface are exercised by their .test.tsx siblings; the animation
        // machinery is verified by tests/e2e/agents-demo.spec.ts which runs
        // against a real browser, not jsdom.
        "src/components/domain/background-beams-with-collision.tsx",
        "src/components/domain/animated-terminal.tsx",
        // IntersectionObserver hook — the fire path requires a real IO
        // instance; jsdom has no meaningful impl. Exercised via the
        // agents-demo Playwright smoke.
        "src/components/domain/use-in-view.ts",
        // Visual DS gallery page. Zero business logic; rendering is
        // verified by tests/e2e/design-system-smoke.spec.ts (42 tests).
        "src/pages/DesignSystemGallery.tsx",
      ],
      thresholds: {
        statements: 85,
        branches: 85,
        functions: 85,
        lines: 85,
      },
    },
  },
});
