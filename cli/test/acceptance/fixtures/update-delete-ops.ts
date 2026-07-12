/**
 * Owner: fleet-update-delete.spec.ts only.
 *
 * Builds an *update* bundle whose front-matter `name:` matches an
 * already-installed fleet, then drives `agentsfleet fleet update <id>
 * --from <dir> --json`. The server's PATCH path enforces a
 * name-equality guard (patch.zig#name_mismatch → UZ-AGT-011): an update
 * bundle whose name differs from the live fleet is rejected. So the only
 * way to exercise the success path is to re-emit the canonical sample
 * with the live fleet's exact name and a mutated SKILL.md body that
 * proves a real config revision bump.
 *
 * This file deliberately does NOT touch the shared seed/lifecycle/teardown
 * fixtures — it composes them via their public exports and adds only the
 * update-bundle construction the lifecycle suite never needed.
 */

import type { RunResult } from "./cli.js";

import crypto from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import url from "node:url";

import {
  PLATFORM_OPS_FIXTURE_NAME,
  PLATFORM_OPS_SAMPLE_DIR,
} from "./constants.ts";
import { runFleetctl } from "./cli.js";

type Env = Readonly<Record<string, string>>;

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(HERE, "..", "..", "..", "..");

const ACCEPTANCE_SLACK_CHANNEL = "#agentsfleet-acceptance";
const ACCEPTANCE_MODEL = "accounts/fireworks/models/kimi-k2.6";
const ACCEPTANCE_CONTEXT_CAP = "256000";
const UPDATE_BUNDLE_DIR_PREFIX = "agentsfleet-update-bundle-";
const SKILL_FILE = "SKILL.md";
const TRIGGER_FILE = "TRIGGER.md";
const UPDATE_TIMEOUT_MS = 120_000;
const NAME_FIELD_PREFIX = "name: ";
const SLACK_TOKEN = "{{slack_channel}}";
const MODEL_TOKEN = "{{model}}";
const CONTEXT_CAP_TOKEN = "{{context_cap_tokens}}";

// A unique, harmless marker appended to the SKILL.md body so the PATCH
// is a genuine source change (config_revision should advance), without
// altering the front-matter the server name-guard reads.
function updateMarker(): string {
  return `\n\n## Acceptance update marker\n\nRevision token: ${crypto.randomBytes(4).toString("hex")}.\n`;
}

// Mirrors seed.ts's substitution map so the update bundle parses through
// the same loadSkillFromPath path the install bundle did — only the name
// is pinned to the live fleet and the body carries the update marker.
async function writeUpdateBundle(fleetName: string): Promise<string> {
  const sourceDir = path.join(WORKTREE_ROOT, PLATFORM_OPS_SAMPLE_DIR);
  const targetDir = await fs.mkdtemp(path.join(os.tmpdir(), UPDATE_BUNDLE_DIR_PREFIX));

  const skill = await fs.readFile(path.join(sourceDir, SKILL_FILE), "utf8");
  const trigger = await fs.readFile(path.join(sourceDir, TRIGGER_FILE), "utf8");

  await fs.writeFile(
    path.join(targetDir, SKILL_FILE),
    skill
      .replace(`${NAME_FIELD_PREFIX}${PLATFORM_OPS_FIXTURE_NAME}`, `${NAME_FIELD_PREFIX}${fleetName}`)
      .replaceAll(SLACK_TOKEN, ACCEPTANCE_SLACK_CHANNEL) + updateMarker(),
  );
  await fs.writeFile(
    path.join(targetDir, TRIGGER_FILE),
    trigger
      .replace(`${NAME_FIELD_PREFIX}${PLATFORM_OPS_FIXTURE_NAME}`, `${NAME_FIELD_PREFIX}${fleetName}`)
      .replaceAll(MODEL_TOKEN, ACCEPTANCE_MODEL)
      .replaceAll(CONTEXT_CAP_TOKEN, ACCEPTANCE_CONTEXT_CAP),
  );
  return targetDir;
}

export interface UpdateResult extends RunResult {
  readonly envelope: { status?: string; fleet_id?: string; config_revision?: unknown };
}

/**
 * Runs `fleet update <id> --from <bundle-with-matching-name> --json` and
 * returns the parsed envelope. Throws on a non-zero exit so the caller
 * can assert the success contract directly. `fleetName` MUST equal the
 * live fleet's name or the server returns UZ-AGT-011.
 */
export async function updateFleetBundle(
  env: Env,
  fleetId: string,
  fleetName: string,
): Promise<UpdateResult> {
  const bundleDir = await writeUpdateBundle(fleetName);
  try {
    const result = await runFleetctl(
      ["fleet", "update", fleetId, "--from", bundleDir, "--json"],
      { env, timeoutMs: UPDATE_TIMEOUT_MS },
    );
    if (result.code !== 0) {
      throw new Error(
        `fleet update ${fleetId} exited ${result.code}: ${result.stderr.trim() || result.stdout.trim()}`,
      );
    }
    const envelope = JSON.parse(result.stdout.trim() || "{}") as UpdateResult["envelope"];
    return { ...result, envelope };
  } finally {
    await fs.rm(bundleDir, { recursive: true, force: true });
  }
}

/** Resolves the live name for a Fleet id from the workspace list. */
export async function resolveFleetName(env: Env, fleetId: string): Promise<string> {
  const result = await runFleetctl(["list", "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`list (name lookup for ${fleetId}) exited ${result.code}: ${result.stderr.trim()}`);
  }
  const payload = JSON.parse(result.stdout.trim() || "{}") as { items?: unknown };
  const items = Array.isArray(payload.items)
    ? (payload.items as Array<{ id?: string; fleet_id?: string; name?: string }>)
    : [];
  const match = items.find((z) => z.id === fleetId || z.fleet_id === fleetId);
  if (!match || typeof match.name !== "string" || match.name.length === 0) {
    throw new Error(`no name found for fleet ${fleetId} in workspace list`);
  }
  return match.name;
}
