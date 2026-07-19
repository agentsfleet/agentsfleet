/**
 * Shared lifecycle action helpers — stop / resume / kill / expectStatus.
 *
 * Each helper composes a `runFleetctl` call, asserts exit 0, and
 * (for status) returns the parsed JSON envelope.
 */

import { runFleetctl } from "./cli.js";

type Env = Readonly<Record<string, string>>;

export interface FleetRow {
  readonly id?: string;
  readonly fleet_id?: string;
  readonly name?: string;
  readonly status?: string;
  readonly workspace_id?: string;
  readonly [key: string]: unknown;
}

async function lifecycleAction(verb: string, fleetId: string, env: Env): Promise<unknown> {
  const result = await runFleetctl([verb, fleetId, "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`${verb} ${fleetId} exited ${result.code}: ${result.stderr.trim()}`);
  }
  return result.stdout.trim() ? JSON.parse(result.stdout.trim()) : null;
}

export const stopFleet = (env: Env, id: string): Promise<unknown> => lifecycleAction("stop", id, env);
export const resumeFleet = (env: Env, id: string): Promise<unknown> => lifecycleAction("resume", id, env);
export const killFleet = (env: Env, id: string): Promise<unknown> => lifecycleAction("kill", id, env);

export async function getStatus(env: Env, fleetId: string, timeoutMs?: number): Promise<FleetRow> {
  // `agentsfleet list --json` returns every fleet in the current workspace.
  // Filter client-side because `agentsfleet status` is workspace-wide.
  const statusArgs = ["list", "--json"];
  const result = await runFleetctl(
    statusArgs,
    timeoutMs === undefined ? { env } : { env, timeoutMs },
  );
  if (result.code !== 0) {
    throw new Error(`list (for status of ${fleetId}) exited ${result.code}: ${result.stderr.trim()}`);
  }
  const payload = JSON.parse(result.stdout.trim() || "{}") as { items?: unknown };
  const items: FleetRow[] = Array.isArray(payload.items) ? (payload.items as FleetRow[]) : [];
  const match = items.find((z) => z.id === fleetId || z.fleet_id === fleetId);
  if (!match) {
    throw new Error(`fleet ${fleetId} not found in workspace list: ${result.stdout.slice(0, 400)}`);
  }
  return match;
}

export async function expectStatus(
  env: Env,
  fleetId: string,
  expected: string | ReadonlyArray<string>,
): Promise<FleetRow> {
  const payload = await getStatus(env, fleetId);
  const actual = payload.status;
  const allowed: ReadonlyArray<string> = Array.isArray(expected) ? expected : [expected as string];
  if (actual === undefined || !allowed.includes(actual)) {
    throw new Error(`expected status ${allowed.join("|")}, got ${actual} for ${fleetId}`);
  }
  return payload;
}
