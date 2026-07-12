/**
 * Acceptance-suite seed helper.
 *
 * Onboards the worktree's canonical sample bundle as a tenant template
 * (`source_kind: "upload"`, via `template-ops.ts`), then drives
 * `agentsfleet install --library <id> --json`. Local-directory install
 * (`--from`) was removed with the two-tier model, so the seed path
 * onboards-then-installs. A unique name per call keeps each onboarded template
 * (and its fleet) distinct. Returns the parsed JSON envelope the CLI emits with
 * `--json` set.
 */

import crypto from "node:crypto";

import { ACCEPTANCE_RUN_PREFIX } from "./constants.ts";
import { runFleetctl } from "./cli.js";
import { ensurePlatformSecretsSeeded } from "./platform-secrets.ts";
import {
  buildPlatformOpsContent,
  onboardUploadTemplate,
  readAuthContext,
} from "./template-ops.ts";

export interface InstallOptions {
  readonly env: Readonly<Record<string, string>>;
  readonly timeoutMs?: number;
  readonly seedFixtureSecrets?: boolean;
  // Defaults to the per-process `ACCEPTANCE_RUN_PREFIX`. Pass a custom
  // prefix only when a spec needs an isolated sub-namespace.
  readonly runPrefix?: string;
}

export interface InstalledFleet {
  readonly id?: string;
  readonly fleet_id?: string;
  readonly [key: string]: unknown;
}

function uniqueName(runPrefix: string): string {
  return `${runPrefix}-platform-ops-${crypto.randomBytes(3).toString("hex")}`;
}

export async function installPlatformOpsFleet(opts: InstallOptions): Promise<InstalledFleet> {
  const runPrefix = opts.runPrefix ?? ACCEPTANCE_RUN_PREFIX;
  const ctx = await readAuthContext(opts.env);
  if (opts.seedFixtureSecrets !== false) await ensurePlatformSecretsSeeded(opts.env);
  const content = await buildPlatformOpsContent(uniqueName(runPrefix));
  const templateId = await onboardUploadTemplate(ctx, content);
  const result = await runFleetctl(
    ["install", "--library", templateId, "--json"],
    { env: opts.env, timeoutMs: opts.timeoutMs ?? 120_000 },
  );
  if (result.code !== 0) {
    throw new Error(`install exited ${result.code}: ${result.stderr.trim() || result.stdout.trim()}`);
  }
  const parsed = JSON.parse(result.stdout.trim()) as InstalledFleet;
  // Both callers fall back via `installed.id ?? installed.fleet_id`; the
  // server's install envelope can carry either key depending on the route.
  if (!parsed.id && !parsed.fleet_id) {
    throw new Error(`install JSON missing id/fleet_id field: ${result.stdout.trim()}`);
  }
  return parsed;
}
