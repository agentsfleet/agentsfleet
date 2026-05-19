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
import fs from "node:fs/promises";
import os from "node:os";

import { ACCEPTANCE_RUN_PREFIX, PLATFORM_OPS_SAMPLE_DIR } from "./constants.ts";
import { runZombiectl } from "./cli.js";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(HERE, "..", "..", "..", "..");
const SAMPLE_NAME = "platform-ops-zombie";

export interface InstallOptions {
  readonly env: Readonly<Record<string, string>>;
  readonly timeoutMs?: number;
  // Defaults to the per-process `ACCEPTANCE_RUN_PREFIX`. Pass a custom
  // prefix only when a spec needs an isolated sub-namespace.
  readonly runPrefix?: string;
}

export interface InstalledZombie {
  readonly id?: string;
  readonly zombie_id?: string;
  readonly [key: string]: unknown;
}

function uniqueName(runPrefix: string): string {
  return `${runPrefix}-platform-ops-${crypto.randomBytes(3).toString("hex")}`;
}

async function createInstallFixture(runPrefix: string): Promise<string> {
  const sourceDir = path.join(WORKTREE_ROOT, PLATFORM_OPS_SAMPLE_DIR);
  const targetDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-platform-ops-"));
  const name = uniqueName(runPrefix);
  const skill = await fs.readFile(path.join(sourceDir, "SKILL.md"), "utf8");
  const trigger = await fs.readFile(path.join(sourceDir, "TRIGGER.md"), "utf8");
  await fs.writeFile(
    path.join(targetDir, "SKILL.md"),
    skill
      .replace(`name: ${SAMPLE_NAME}`, `name: ${name}`)
      .replaceAll("{{slack_channel}}", "#usezombie-acceptance"),
  );
  await fs.writeFile(
    path.join(targetDir, "TRIGGER.md"),
    trigger
      .replace(`name: ${SAMPLE_NAME}`, `name: ${name}`)
      .replaceAll("{{model}}", "accounts/fireworks/models/kimi-k2.6")
      .replaceAll("{{context_cap_tokens}}", "256000"),
  );
  return targetDir;
}

export async function installPlatformOpsZombie(opts: InstallOptions): Promise<InstalledZombie> {
  const samplePath = await createInstallFixture(opts.runPrefix ?? ACCEPTANCE_RUN_PREFIX);
  const result = await runZombiectl(
    ["install", "--from", samplePath, "--json"],
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
