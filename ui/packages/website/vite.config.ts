import { createElement } from "react";
import { renderToString } from "react-dom/server";
import Home from "./src/pages/Home";
import { structuredData } from "./src/landing/metadata";
import { DEFAULT_WAITLIST_URL } from "./src/lib/site-links";
import { resolve } from "node:path";
import { defineConfig, loadEnv, type Connect } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const TEST_FILE_PATTERN = "src/**/*.test.{ts,tsx}" as const;

// The homepage ships its complete content as HTML. The other page URLs use
// the React Router entry, just as they do behind the production rewrite.
const routeApplication: Connect.NextHandleFunction = (
  request,
  _response,
  next,
) => {
  const pathname = request.url?.split("?")[0];
  if (
    pathname &&
    /^\/(?!@)[^.?]+$/.test(pathname) &&
    request.headers.accept?.includes("text/html")
  ) {
    request.url = "/app.html";
  }
  next();
};

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "VITE_");
  const signup =
    env.VITE_WAITLIST_URL?.trim() ||
    DEFAULT_WAITLIST_URL;
  return {
    appType: "mpa",
    plugins: [
      tailwindcss(),
      react(),
      {
        name: "website-pages",
        configureServer(server) {
          server.middlewares.use(routeApplication);
        },
        configurePreviewServer(server) {
          server.middlewares.use(routeApplication);
        },
        transformIndexHtml(html) {
          if (!html.includes("<!--landing-page-->")) return html;
          const page = renderToString(createElement(Home, { waitlistUrl: signup }));
          const schema = JSON.stringify(structuredData).replaceAll("<", "\\u003c");
          return html.replace("<!--landing-page-->", () => page)
            .replace("<!--landing-schema-->", () => `<script type="application/ld+json">${schema}</script>`);
        },
      },
    ],
    build: {
      rollupOptions: {
        input: {
          index: resolve(import.meta.dirname, "index.html"),
          app: resolve(import.meta.dirname, "app.html"),
        },
      },
    },
    test: {
      // Keep Vitest workers on threads under the pinned Bun runtime.
      pool: "threads",
      environment: "jsdom",
      globals: true,
      setupFiles: ["../design-system/src/test-setup.ts", "./src/test-setup.ts"],
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
          "src/landing/entry.tsx",
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
  };
});
