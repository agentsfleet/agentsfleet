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

  // Same-origin proxy for API calls. Browser hits /backend/v1/... (no CORS);
  // Next.js server forwards to the real backend.
  async rewrites() {
    const backend = process.env.API_BACKEND_URL ?? "https://api-dev.usezombie.com";
    return [{ source: "/backend/:path*", destination: `${backend}/:path*` }];
  },
};

export default nextConfig;
