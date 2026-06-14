#!/usr/bin/env node
//
// Pre-publish copier — bundles `samples/` from the repo root into the npm
// package directory so it ships inside the published tarball. Agent skills
// live in their own repo (github.com/usezombie/skills) and install via
// `npx skills add usezombie/skills` — they no longer travel
// with @usezombie/zombiectl.
//
// Why a copy rather than a symlink in `files:`? npm `files:` only resolves
// relative to the package root; it cannot reference parent paths. The
// authoritative copy stays at repo root (shared across agentsfleet + other
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

// Defense-in-depth: scrub a stray local `skills/` that used to be bundled
// but no longer should be. gitignore + `files:` already guard it from
// publishing, but a manual `cpSync` in a stale local shell session
// could resurrect it. Active rm at publish-time closes that hole.
const staleSkills = resolve(pkgRoot, "skills");
if (existsSync(staleSkills)) {
  rmSync(staleSkills, { recursive: true, force: true });
  console.log(`prepublish: scrubbed stale skills/ from ${pkgRoot}`);
}

const samplesSrc = resolve(repoRoot, "samples");
const samplesDst = resolve(pkgRoot, "samples");
if (!existsSync(samplesSrc)) {
  console.error(`prepublish: source ${samplesSrc} missing — refusing to publish a package without samples/`);
  process.exit(1);
}
if (existsSync(samplesDst)) rmSync(samplesDst, { recursive: true, force: true });
cpSync(samplesSrc, samplesDst, { recursive: true });
console.log(`prepublish: copied samples/ (${samplesSrc} → ${samplesDst})`);
