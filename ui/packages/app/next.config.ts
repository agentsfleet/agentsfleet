import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Turbopack is the default bundler in Next.js 16.1.
  // File system cache (incremental computation) is stable and on-by-default —
  // dependency graphs, aggregation graphs, and value cells persist to disk.
  // No turbo config block needed; it's all automatic.
  // See: https://nextjs.org/blog/turbopack-incremental-computation

  // React 19 compiler — off until codebase is fully annotated.
  // Moved to top-level in Next.js 16.1 (was experimental.reactCompiler).
  reactCompiler: false,

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
    // dev API. Fail the boot loudly instead — the post-checkout hook links
    // .env.local from ~/.config/agentsfleet/ui.env.local (Next loads .env
    // files before this config runs).
    const backend = process.env.NEXT_PUBLIC_API_URL;
    if (!backend) {
      throw new Error(
        "NEXT_PUBLIC_API_URL is unset — run provision-env-1password; the post-checkout hook links ui/packages/app/.env.local.",
      );
    }
    return [{ source: "/backend/:path*", destination: `${backend}/:path*` }];
  },
};

export default nextConfig;
