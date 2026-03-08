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
};

export default nextConfig;
