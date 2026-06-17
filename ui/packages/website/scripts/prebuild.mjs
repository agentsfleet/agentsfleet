#!/usr/bin/env node
//
// Pre-build website asset prep — places `public/openapi.json` (the canonical
// OpenAPI bundle at repo root) into the website package's `public/` and writes
// the llms.txt / llms-full.txt text surfaces so Vite picks them up at
// static-asset bundling time.
//
// Why a script file rather than an inline `node -e` in package.json?
// `bun run <script>` in a workspace package runs the script with
// cwd = workspace root, not the package dir. An inline `../../../public/...`
// resolved from the package dir, but from the workspace root that path
// climbs above the repo and ENOENTs. Resolving from `import.meta.url`
// makes the source path independent of whatever cwd the runner picks.

import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { DOCS_URL, GITHUB_URL, INSTALL_COMMAND, MARKETING_SITE_URL } from "../src/config";
import { buildLlmsFullText, buildLlmsIndexText } from "../src/lib/llms-text";
import { RATES_DISPLAY } from "../src/lib/rates";

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "..", "..", "..");

const src = resolve(repoRoot, "public", "openapi.json");
const dstDir = resolve(pkgRoot, "public");
const dst = resolve(dstDir, "openapi.json");
const llmsTxt = resolve(dstDir, "llms.txt");
const llmsFullTxt = resolve(dstDir, "llms-full.txt");

// `ui/packages/website/public/` may not exist on a fresh checkout — git
// doesn't track empty directories, and the legacy v1 public files
// (agent-manifest.json, heartbeat, llms.txt, skill.md) were removed in
// the M49_001 v1-surface cleanup. Recreate the dir before the copy so
// neither dev checkouts nor Continuous Integration (CI) ENOENT here.
mkdirSync(dstDir, { recursive: true });
await Bun.write(dst, Bun.file(src));

const llmsInputs = {
  docsUrl: DOCS_URL,
  githubUrl: GITHUB_URL,
  installCommand: INSTALL_COMMAND,
  siteUrl: MARKETING_SITE_URL,
  runRatePerSecond: RATES_DISPLAY.RUN_RATE_PER_SEC,
  starterCredit: RATES_DISPLAY.STARTER_CREDIT,
  eventRate: RATES_DISPLAY.EVENT_RATE,
};

await Bun.write(llmsTxt, buildLlmsIndexText(llmsInputs));
await Bun.write(llmsFullTxt, buildLlmsFullText(llmsInputs));
