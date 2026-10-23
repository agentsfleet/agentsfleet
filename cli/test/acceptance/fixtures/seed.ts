// Native fixture setup exercises the terminal Review and explicit Save commands.

import crypto from "node:crypto";

import {
  ACCEPTANCE_RUN_PREFIX,
  AGENTSFLEET_STATUS,
  TERMINAL_STATUSES,
} from "./constants.ts";
import { FleetNotFoundError, getStatus } from "./lifecycle.ts";
import { ensurePlatformSecretsSeeded } from "./platform-secrets.ts";
import { fixtureDefinition, saveNativeFleet } from "./native-setup.ts";

export interface SaveOptions {
  readonly env: Readonly<Record<string, string>>;
  readonly timeoutMs?: number;
  readonly seedFixtureSecrets?: boolean;
  readonly runPrefix?: string;
  // An explicit runtime reference for refusal/recovery journeys.
  readonly model?: string;
}

export interface SavedFleet {
  readonly id: string;
  readonly [key: string]: unknown;
}

const FLEET_READY_POLL_MS = 1_000;
const DEFAULT_SAVE_TIMEOUT_MS = 120_000;

function uniqueName(runPrefix: string, fixtureName: string): string {
  return `${runPrefix}-${fixtureName}-${crypto.randomBytes(3).toString("hex")}`;
}

function remainingTimeoutMs(deadline: number): number {
  return Math.max(1, deadline - Date.now());
}

function fleetReachedActive(fleetId: string, status: string | undefined): boolean {
  if (status === AGENTSFLEET_STATUS.active) return true;
  if (status !== undefined && TERMINAL_STATUSES.includes(status)) {
    throw new Error(`fleet ${fleetId} entered terminal status=${status} before becoming active`);
  }
  return false;
}

function fleetReadErrorIsRetryable(error: unknown): boolean {
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

export async function saveProbeFleet(opts: SaveOptions): Promise<SavedFleet> {
  if (opts.seedFixtureSecrets) await ensurePlatformSecretsSeeded(opts.env, () => opts.timeoutMs ?? DEFAULT_SAVE_TIMEOUT_MS);
  const name = uniqueName(opts.runPrefix ?? ACCEPTANCE_RUN_PREFIX, "probe");
  return saveNativeFleet(opts.env, fixtureDefinition(name), opts.model);
}

export async function saveSteerProbeFleet(opts: SaveOptions): Promise<SavedFleet> {
  const fleet = await saveProbeFleet(opts);
  const deadline = Date.now() + (opts.timeoutMs ?? DEFAULT_SAVE_TIMEOUT_MS);
  await waitForFleetActive(opts.env, fleet.id, deadline);
  return fleet;
}
