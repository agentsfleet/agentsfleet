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

import {
  ACCEPTANCE_RUN_PREFIX,
  AGENTSFLEET_STATUS,
  TERMINAL_STATUSES,
} from "./constants.ts";
import { runFleetctl } from "./cli.js";
import { FleetNotFoundError, getStatus } from "./lifecycle.ts";
import { ensurePlatformSecretsSeeded } from "./platform-secrets.ts";
import {
  buildPlatformOpsContent,
  onboardUploadTemplate,
  readAuthContext,
} from "./template-ops.ts";

export interface InstallOptions {
  readonly env: Readonly<Record<string, string>>;
  // Total budget for onboarding, install, and readiness polling.
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

const FLEET_READY_POLL_MS = 1_000;
const DEFAULT_INSTALL_TIMEOUT_MS = 120_000;

function uniqueName(runPrefix: string): string {
  return `${runPrefix}-platform-ops-${crypto.randomBytes(3).toString("hex")}`;
}

function remainingTimeoutMs(deadline: number): number {
  return Math.max(1, deadline - Date.now());
}

export function fleetReachedActive(fleetId: string, status: string | undefined): boolean {
  if (status === AGENTSFLEET_STATUS.active) return true;
  if (status !== undefined && TERMINAL_STATUSES.includes(status)) {
    throw new Error(`fleet ${fleetId} entered terminal status=${status} before becoming active`);
  }
  return false;
}

export function fleetReadErrorIsRetryable(error: unknown): boolean {
  return error instanceof FleetNotFoundError;
}

async function waitForFleetActive(
  env: Readonly<Record<string, string>>,
  fleetId: string,
  deadline: number,
): Promise<void> {
  let lastStatus: string | undefined;
  while (Date.now() < deadline) {
    try {
      const fleet = await getStatus(env, fleetId, remainingTimeoutMs(deadline));
      lastStatus = fleet.status;
      if (fleetReachedActive(fleetId, lastStatus)) return;
    } catch (error) {
      if (!fleetReadErrorIsRetryable(error)) throw error;
    }
    await Bun.sleep(Math.min(FLEET_READY_POLL_MS, remainingTimeoutMs(deadline)));
  }
  throw new Error(`fleet ${fleetId} did not become active; last status=${lastStatus ?? "unknown"}`);
}

export async function installPlatformOpsFleet(opts: InstallOptions): Promise<InstalledFleet> {
  const deadline = Date.now() + (opts.timeoutMs ?? DEFAULT_INSTALL_TIMEOUT_MS);
  const runPrefix = opts.runPrefix ?? ACCEPTANCE_RUN_PREFIX;
  const ctx = await readAuthContext(opts.env);
  if (opts.seedFixtureSecrets !== false) {
    await ensurePlatformSecretsSeeded(opts.env, () => remainingTimeoutMs(deadline));
  }
  const content = await buildPlatformOpsContent(uniqueName(runPrefix));
  const templateId = await onboardUploadTemplate(ctx, content, remainingTimeoutMs(deadline));
  const result = await runFleetctl(
    ["install", "--library", templateId, "--json"],
    { env: opts.env, timeoutMs: remainingTimeoutMs(deadline) },
  );
  if (result.code !== 0) {
    throw new Error(`install exited ${result.code}: ${result.stderr.trim() || result.stdout.trim()}`);
  }
  const parsed = JSON.parse(result.stdout.trim()) as InstalledFleet;
  // Both callers fall back via `installed.id ?? installed.fleet_id`; the
  // server's install envelope can carry either key depending on the route.
  const fleetId = parsed.id ?? parsed.fleet_id;
  if (!fleetId) {
    throw new Error(`install JSON missing id/fleet_id field: ${result.stdout.trim()}`);
  }
  await waitForFleetActive(opts.env, fleetId, deadline);
  return parsed;
}
