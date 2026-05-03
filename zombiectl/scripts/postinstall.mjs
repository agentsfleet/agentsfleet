#!/usr/bin/env node
//
// Post-install copier — places the bundled zombie-template `samples/` tree
// at a stable user-local path (~/.config/usezombie/samples/) so the agent
// skills (e.g. /usezombie-install-platform-ops) can read the canonical
// templates from a known location instead of fetching over the network.
//
// Defensive contract: this script never crashes `npm install`. Any FS or
// permission error is logged as a warning and the process exits 0. The
// skill's missing-samples failure mode catches the consequence at
// invocation time and tells the user how to repair (re-run `npm install
// -g @usezombie/zombiectl`).
//
// Idempotency: a sha256 manifest of the source tree is written alongside
// the copy. On subsequent installs we compare manifests and skip the
// copy when the source hasn't changed — keeps `npm install` fast on
// repeated `@latest` upgrades that didn't bump the templates.

import {
  cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync,
  statSync, writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { dirname, resolve, join } from "node:path";
import { homedir } from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const src = resolve(pkgRoot, "samples");
const dstParent = resolve(homedir(), ".config", "usezombie");
const dst = resolve(dstParent, "samples");
const manifestPath = resolve(dstParent, ".samples-manifest");

function warn(msg) {
  console.warn(`@usezombie/zombiectl postinstall: ${msg} — skipping. Re-run \`npm install -g @usezombie/zombiectl\` if templates are missing.`);
}

function manifestOf(root) {
  // Cheap, deterministic: walk in sorted order, hash relative-path + size.
  // Mtimes deliberately excluded — git checkouts and tarball extracts both
  // smear them, which would make the manifest churn on every install.
  const h = createHash("sha256");
  function walk(d, rel = "") {
    const entries = readdirSync(d, { withFileTypes: true })
      .sort((a, b) => a.name.localeCompare(b.name));
    for (const e of entries) {
      const r = rel ? `${rel}/${e.name}` : e.name;
      const full = join(d, e.name);
      if (e.isDirectory()) {
        h.update(`d:${r}\n`);
        walk(full, r);
      } else if (e.isFile()) {
        h.update(`f:${r}:${statSync(full).size}\n`);
      }
    }
  }
  walk(root);
  return h.digest("hex");
}

try {
  // Source missing means we're running inside a dev checkout of the repo,
  // not from a tarball. The prepublish step bundles `samples/` into the
  // package directory; if it's not there, there's nothing to copy.
  if (!existsSync(src)) {
    // No warning — this is normal during local `npm install` in a dev
    // checkout where prepublish hasn't run.
    process.exit(0);
  }

  const newManifest = manifestOf(src);
  const oldManifest = existsSync(manifestPath)
    ? readFileSync(manifestPath, "utf8").trim()
    : "";

  if (existsSync(dst) && newManifest === oldManifest) {
    // Up-to-date. Skip the copy entirely.
    process.exit(0);
  }

  mkdirSync(dstParent, { recursive: true });

  if (existsSync(dst)) {
    const ts = new Date().toISOString().replace(/[:.]/g, "-");
    // Append pid so two concurrent `npm install -g` runs landing in the
    // same millisecond can't collide on the backup directory name.
    const backup = resolve(dstParent, `samples.backup-${ts}-${process.pid}`);
    cpSync(dst, backup, { recursive: true });
    rmSync(dst, { recursive: true, force: true });
  }

  cpSync(src, dst, { recursive: true });
  writeFileSync(manifestPath, newManifest + "\n");
  console.log(`@usezombie/zombiectl postinstall: samples installed at ${dst}`);
} catch (err) {
  warn(err && err.message ? err.message : String(err));
  process.exit(0); // never crash npm install
}
