#!/usr/bin/env node
//
// Pre-publish copier — bundles `samples/` and `skills/` from the repo root
// into the npm package directory so they ship inside the published tarball.
//
// Why a copy rather than a symlink in `files:`? npm `files:` only resolves
// relative to the package root; it cannot reference parent paths. The
// authoritative copies stay at repo root (shared across zombiectl + other
// tooling); we mirror them in here at publish time, then `npm publish`
// includes them via the `files:` entry.
//
// Both target directories are gitignored — they exist only in the
// published tarball and the local `npm pack` output.

import { cpSync, rmSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "..");

for (const name of ["samples", "skills"]) {
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
