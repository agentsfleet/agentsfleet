#!/usr/bin/env node
//
// Pre-publish copier — bundles `samples/` from the repo root into the npm
// package directory so it ships inside the published tarball. Agent skills
// live in their own repo (github.com/usezombie/skills) since M69_001 and
// install via `npx skills add usezombie/skills` — they no longer travel
// with @usezombie/zombiectl.
//
// Why a copy rather than a symlink in `files:`? npm `files:` only resolves
// relative to the package root; it cannot reference parent paths. The
// authoritative copy stays at repo root (shared across zombiectl + other
// tooling); we mirror it here at publish time, then `npm publish` includes
// it via the `files:` entry.
//
// The target directory is gitignored — it exists only in the published
// tarball and the local `npm pack` output.

import { cpSync, rmSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "..");

for (const name of ["samples"]) {
  const src = resolve(repoRoot, name);
  const dst = resolve(pkgRoot, name);
  if (!existsSync(src)) {
    console.error(`prepublish: source ${src} missing — refusing to publish a package without ${name}/`);
    process.exit(1);
  }
  if (existsSync(dst)) rmSync(dst, { recursive: true, force: true });
  cpSync(src, dst, { recursive: true });
  console.log(`prepublish: copied ${name}/ (${src} → ${dst})`);
}
