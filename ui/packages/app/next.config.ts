import type { NextConfig } from "next";
import { loadEnvFile } from "node:process";

const uiEnvFile = process.env.AGENTSFLEET_UI_ENV_FILE;
if (uiEnvFile) loadEnvFile(uiEnvFile);

const buildId = process.env.AGENTSFLEET_BUILD_ID;
if (buildId && !/^[0-9a-f]{64}$/.test(buildId)) throw new Error("Invalid build source identity");

const nextConfig: NextConfig = {
  generateBuildId: async () => buildId ?? null,
  // Compiled into Next's routes manifest; changing the serving shell cannot restamp a build.
  headers: async () => buildId ? [{
    source: "/:path*", headers: [{ key: "x-agentsfleet-build", value: buildId }],
  }] : [],
  // Turbopack is the default bundler in Next.js 16.1.
  // File system cache (incremental computation) is stable and on-by-default —
  // dependency graphs, aggregation graphs, and value cells persist to disk.
  // No turbo config block needed; it's all automatic.
  // See: https://nextjs.org/blog/turbopack-incremental-computation

  // Start with explicitly annotated components so compiled behavior can be
  // compared before expanding to the rest of the dashboard.
  reactCompiler: {
    compilationMode: "annotation",
    environment: { enablePreserveExistingMemoizationGuarantees: true },
  },

  // Strict TypeScript checks during build.
  typescript: {
    ignoreBuildErrors: false,
  },

  // TypeScript 7 ships a Go-native compiler with no lib/typescript.js, so the
  // build must shell out to the project-local tsc CLI instead of loading the
  // retired JavaScript compiler API (vercel/next.js#95831, stable in 16.2.12).
  experimental: {
    useTypeScriptCli: true,
  },

  // Same-origin proxy for API calls. Browser hits /backend/v1/... (no CORS);
  // Next.js server forwards to the real backend. Uses the same env var as
  // lib/api/client.ts so a single value drives both server-side and
  // browser-side fetches — no possibility of them routing to different
  // backends in prod.
  //
  // The token-minting Route Handlers deliberately live under `/live/*`, NOT
  // under this prefix. When they shared it, Vercel's edge router applied the
  // rewrite ahead of the same-prefix filesystem routes, so EventSource
  // requests reached the API carrying only a cookie and no Bearer — a 401
  // UZ-AUTH-002 on every stream connect, invisible locally where Next
  // resolves the handler first. A non-overlapping prefix removes the
  // precedence question entirely instead of relying on router ordering.
  async rewrites() {
    // No fallback: an env-less worktree once silently proxied to the shared
    // dev API. Fail the boot loudly instead. Local test shells set
    // AGENTSFLEET_UI_ENV_FILE to the machine-level UI env file.
    const backend = process.env.NEXT_PUBLIC_API_URL;
    if (!backend) {
      throw new Error(
        "NEXT_PUBLIC_API_URL is unset — run provision-env-1password and set AGENTSFLEET_UI_ENV_FILE.",
      );
    }
    return [{ source: "/backend/:path*", destination: `${backend}/:path*` }];
  },
};

export default nextConfig;
