/**
 * Fixture helpers exclusive to `install-negatives.spec.ts`.
 *
 * Two distinct bundle needs, two distinct builders:
 *
 *   1. Malformed bundles (missing SKILL.md) and a
 *      nonexistent path — these only ever exercise the Command-Line Interface (CLI)'s *client-side*
 *      loader (`loadSkillFromPath`), which throws its typed
 *      `SkillLoadError` codes BEFORE any HTTP call. The bundle contents
 *      are irrelevant past "which file is absent", so these stay minimal.
 *
 *   2. A *complete, server-valid* bundle with a STABLE prefixed name — for
 *      the duplicate-name path. The server rejects an under-specified
 *      bundle long before it reaches the `(workspace_id, name)` uniqueness
 *      constraint: SKILL.md frontmatter requires `name` + `description` +
 *      `version`, and TRIGGER.md requires a full `x-agentsfleet:` runtime
 *      block (triggers, budget.daily_dollars, …). A hand-rolled "name:
 *      only" bundle therefore fails with ERR_AGENTSFLEET_INVALID_CONFIG and
 *      the duplicate conflict is never reached. So the named builder copies
 *      the canonical `platform-ops-sample` bundle and rewrites only the
 *      frontmatter `name:` (to a stable, prefixed value) plus the two
 *      frontmatter template placeholders — mirroring `seed.ts`'s
 *      `createInstallFixture`, which is the one proven-good install path.
 *
 * Every emitted name is prefixed so `cleanWorkspaceFleets` reclaims any
 * fleet this run actually managed to create.
 */

import crypto from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import url from "node:url";

import { ACCEPTANCE_RUN_PREFIX, PLATFORM_OPS_SAMPLE_DIR } from "./constants.ts";

// Exit codes the CLI maps client-side error tags to (mirrors
// `cli/src/errors/index.ts` EXIT_CODE). Repeated across assertions →
// named here per RULE UFS.
export const EXIT_CONFIG_ERROR = 5;
export const EXIT_VALIDATION_ERROR = 4;
export const EXIT_SERVER_ERROR = 3;

// Typed codes `loadSkillFromPath` throws (mirrors
// `cli/src/lib/load-skill-from-path.ts`). The CLI renders the code into
// stderr via `ConfigError.message` (`<code>: <detail>`).
export const ERR_PATH_NOT_FOUND = "ERR_PATH_NOT_FOUND";
export const ERR_SKILL_MISSING = "ERR_SKILL_MISSING";

// Server conflict code for a duplicate fleet name within a workspace
// (mirrors `core` schema `uq_fleets_workspace_id_name` →
// `error_entries.zig` / `error_registry.zig` UZ-AGT-006, a 409). The CLI
// surfaces it as a ServerError (exit 3) carrying this code in stderr.
export const ERR_AGENTSFLEET_NAME_TAKEN = "UZ-AGT-006";

// Frontmatter constants from the canonical sample bundle. The sample's
// `name:` line and the two frontmatter template tokens are rewritten on
// copy so the bundle parses server-side. Body-only placeholders
// (`{{slack_channel}}`, `{{cron_schedule}}`, …) live outside frontmatter
// and never reach the config parser, so they are left untouched — exactly
// as `seed.ts` does.
const SAMPLE_NAME = "platform-ops-fleet";
const FRONTMATTER_NAME_LINE = `name: ${SAMPLE_NAME}`;
const TOKEN_MODEL = "{{model}}";
const TOKEN_CONTEXT_CAP = "{{context_cap_tokens}}";
const SAMPLE_MODEL = "accounts/fireworks/models/kimi-k2.6";
const SAMPLE_CONTEXT_CAP = "256000";
const SKILL_FILENAME = "SKILL.md";
const TRIGGER_FILENAME = "TRIGGER.md";
const UTF8 = "utf8";

const TMP_PREFIX = "agentsfleet-install-neg-";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
// fixtures/ → test/acceptance/ → test/ → cli/ → worktree root.
const WORKTREE_ROOT = path.resolve(HERE, "..", "..", "..", "..");

function uniqueName(runPrefix: string, slug: string): string {
  return `${runPrefix}-${slug}-${crypto.randomBytes(3).toString("hex")}`;
}

async function mkBundleDir(): Promise<string> {
  return fs.mkdtemp(path.join(os.tmpdir(), TMP_PREFIX));
}

export interface NamedBundle {
  readonly dir: string;
  readonly name: string;
}

/**
 * A complete, SERVER-VALID bundle with a STABLE prefixed name, built by
 * copying the canonical `platform-ops-sample` and rewriting only the
 * frontmatter `name:` plus the two frontmatter template tokens. Installing
 * the returned dir twice trips `uq_fleets_workspace_id_name` → UZ-AGT-006.
 */
export async function makeNamedBundle(runPrefix = ACCEPTANCE_RUN_PREFIX): Promise<NamedBundle> {
  const sourceDir = path.join(WORKTREE_ROOT, PLATFORM_OPS_SAMPLE_DIR);
  const dir = await mkBundleDir();
  const name = uniqueName(runPrefix, "dup");

  const skill = await fs.readFile(path.join(sourceDir, SKILL_FILENAME), UTF8);
  const trigger = await fs.readFile(path.join(sourceDir, TRIGGER_FILENAME), UTF8);

  await fs.writeFile(
    path.join(dir, SKILL_FILENAME),
    skill.replace(FRONTMATTER_NAME_LINE, `name: ${name}`),
  );
  await fs.writeFile(
    path.join(dir, TRIGGER_FILENAME),
    trigger
      .replace(FRONTMATTER_NAME_LINE, `name: ${name}`)
      .replaceAll(TOKEN_MODEL, SAMPLE_MODEL)
      .replaceAll(TOKEN_CONTEXT_CAP, SAMPLE_CONTEXT_CAP),
  );
  return { dir, name };
}

/**
 * A directory that exists but is missing SKILL.md → ERR_SKILL_MISSING.
 * Only TRIGGER.md is written so the loader passes the directory check and
 * fails specifically on the absent skill file. Contents never reach the
 * server (the loader throws first), so a stub body is enough.
 */
export async function makeSkillMissingBundle(): Promise<string> {
  const dir = await mkBundleDir();
  await fs.writeFile(path.join(dir, TRIGGER_FILENAME), "# trigger stub\n", { mode: 0o644 });
  return dir;
}


/**
 * An absolute path guaranteed not to exist on disk → ERR_PATH_NOT_FOUND.
 * Built under the OS tmp dir with a random tail so a stale leftover can
 * never satisfy it.
 */
export function nonexistentBundlePath(): string {
  return path.join(os.tmpdir(), `${TMP_PREFIX}absent-${crypto.randomBytes(6).toString("hex")}`);
}

export async function removeDir(dir: string | null | undefined): Promise<void> {
  if (!dir) return;
  await fs.rm(dir, { recursive: true, force: true });
}
