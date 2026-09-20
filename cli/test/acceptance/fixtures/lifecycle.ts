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

export class FleetNotFoundError extends Error {}

// An illegal lifecycle transition must be REFUSED, and the daemon answers one
// of TWO codes — never either. `edit.rs`'s `explain` re-reads the row a
// zero-row UPDATE left behind: a killed row is a tombstone and answers
// `ErrorKind::NotFound` (UZ-AGT-009, a 404), and everything else the status
// machine turned down answers `ErrorKind::TransitionRefused` (UZ-AGT-010, a
// 409). `purge.rs` refuses a delete of a fleet nobody killed first with
// `ErrorKind::MustKillFirst`, which maps to UZ-AGT-010 as well. Both mappings
// live in `afd_fleet_lifecycle/src/error.rs`.
//
// One pattern PER CLASSIFICATION, not one shared alternation. A regex that
// accepts both codes everywhere still passes when the daemon picks the wrong
// one, and the state-only assertions that follow each refusal cannot see the
// difference — the fleet lands in the right state either way, so a
// misclassification would ship unnoticed.
//
// Each pattern carries the sentence alongside its own code because that is the
// half a person reads, and it is the half that moved: the CLI now renders the
// daemon's `user_message` ("We couldn't find that Fleet") where it used to
// print the log-side `detail` ("fleet not found"). A regex that knew only the
// old wording went red on a rendering change and named nothing about the
// product.

/** A killed row is a tombstone: resume-of-killed and kill-of-killed. */
export const TOMBSTONE_REFUSAL =
  /UZ-AGT-009|couldn't find that Fleet|HTTP_404|Not Found/i;

/** The status machine turned the transition down: delete-before-kill and
 *  stop-already-stopped. */
export const TRANSITION_REFUSAL =
  /UZ-AGT-010|transition not allowed|already.*terminal|must be killed|HTTP_409|Conflict/i;

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
    throw new FleetNotFoundError(
      `fleet ${fleetId} not found in workspace list: ${result.stdout.slice(0, 400)}`,
    );
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
