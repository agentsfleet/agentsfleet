#!/usr/bin/env node
//
// Pre-build copier — places `public/openapi.json` (the canonical OpenAPI
// bundle at repo root) into the website package's `public/` so Vite picks
// it up at static-asset bundling time.
//
// Why a script file rather than an inline `node -e` in package.json?
// `bun run <script>` in a workspace package runs the script with
// cwd = workspace root, not the package dir. An inline `../../../public/...`
// resolved from the package dir, but from the workspace root that path
// climbs above the repo and ENOENTs. Resolving from `import.meta.url`
// makes the source path independent of whatever cwd the runner picks.

import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "..", "..", "..");

const src = resolve(repoRoot, "public", "openapi.json");
const dstDir = resolve(pkgRoot, "public");
const dst = resolve(dstDir, "openapi.json");

// `ui/packages/website/public/` may not exist on a fresh checkout — git
// doesn't track empty directories, and the legacy v1 public files
// (agent-manifest.json, heartbeat, llms.txt, skill.md) were removed in
// the M49_001 v1-surface cleanup. Recreate the dir before the copy so
// neither dev checkouts nor CI ENOENT here.
mkdirSync(dstDir, { recursive: true });
copyFileSync(src, dst);
console.log(`prebuild: copied ${src} → ${dst}`);
