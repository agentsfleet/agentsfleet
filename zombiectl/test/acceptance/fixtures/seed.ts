/**
 * Acceptance-suite seed helper.
 *
 * Drives `zombiectl install --from samples/platform-ops` against the
 * worktree's canonical sample bundle. Returns the parsed JSON envelope
 * the CLI emits with `--json` set.
 */

import crypto from "node:crypto";
import path from "node:path";
import url from "node:url";

import { PLATFORM_OPS_SAMPLE_DIR } from "./constants.ts";
import { runZombiectl } from "./cli.js";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(HERE, "..", "..", "..", "..");

export interface InstallOptions {
  readonly env: Readonly<Record<string, string>>;
  readonly name?: string;
  readonly timeoutMs?: number;
}

export interface InstalledZombie {
  readonly id?: string;
  readonly zombie_id?: string;
  readonly [key: string]: unknown;
}

function uniqueName(prefix: string): string {
  return `${prefix}-${crypto.randomBytes(4).toString("hex")}`;
}

export async function installPlatformOpsZombie(opts: InstallOptions): Promise<InstalledZombie> {
  const name = opts.name ?? uniqueName("platform-ops-acceptance");
  const samplePath = path.join(WORKTREE_ROOT, PLATFORM_OPS_SAMPLE_DIR);
  const result = await runZombiectl(
    ["install", "--from", samplePath, "--name", name, "--json"],
    { env: opts.env, timeoutMs: opts.timeoutMs ?? 120_000 },
  );
  if (result.code !== 0) {
    throw new Error(`install exited ${result.code}: ${result.stderr.trim() || result.stdout.trim()}`);
  }
  const parsed = JSON.parse(result.stdout.trim()) as InstalledZombie;
  // Both callers fall back via `installed.id ?? installed.zombie_id`; the
  // server's install envelope can carry either key depending on the route.
  if (!parsed.id && !parsed.zombie_id) {
    throw new Error(`install JSON missing id/zombie_id field: ${result.stdout.trim()}`);
  }
  return parsed;
}
