/**
 * Shared lifecycle action helpers — stop / resume / kill / expectStatus.
 *
 * Each helper composes a `runFleetctl` call, asserts exit 0, and
 * (for status) returns the parsed JSON envelope.
 */

import { runFleetctl } from "./cli.js";
import { Schema } from "effect";
import { FleetSchema, decimal } from "../../../src/commands/chat/protocol.ts";

type Env = Readonly<Record<string, string>>;

const FleetDetailSchema = Schema.Struct({
  ...FleetSchema.fields, events_processed: decimal, budget_used_nanos: decimal,
});

export interface FleetRow {
  readonly id?: string;
  readonly fleet_id?: string;
  readonly name?: string;
  readonly status?: string;
  readonly workspace_id?: string;
  readonly [key: string]: unknown;
}

export class FleetNotFoundError extends Error {}

// An archived fleet refuses every further transition as a state conflict:
// `transition` in `afd_authoring/src/creation/patch.rs` answers UZ-AGT-020, a
// 409 carrying `current_state: archived`, while a GET still returns the fleet.
// Delete has no status precondition, so it has no refusal of its own.
//
// The pattern carries the sentence alongside its code because that is the
// half a person reads: the CLI renders the daemon's `user_message`.

/** An archived fleet refuses every further transition. */
export const ARCHIVED_REFUSAL = /UZ-AGT-020|Fleet is archived|HTTP_409/i;

async function lifecycleAction(verb: string, fleetId: string, env: Env): Promise<unknown> {
  const result = await runFleetctl(["fleet", verb, fleetId, "--json"], { env });
  if (result.code !== 0) {
    throw new Error(`${verb} ${fleetId} exited ${result.code}: ${result.stderr.trim()}`);
  }
  return result.stdout.trim() ? JSON.parse(result.stdout.trim()) : null;
}

export const archiveFleet = (env: Env, id: string): Promise<unknown> => lifecycleAction("archive", id, env);

export async function getStatus(env: Env, fleetId: string, timeoutMs?: number): Promise<typeof FleetDetailSchema.Type> {
  const result = await runFleetctl(
    ["fleet", "show", fleetId, "--json"],
    timeoutMs === undefined ? { env } : { env, timeoutMs },
  );
  if (result.code !== 0) {
    if (/UZ-AGT-009|HTTP_404/.test(result.stderr)) {
      throw new FleetNotFoundError(`fleet ${fleetId} is unavailable`);
    }
    throw new Error(`fleet show ${fleetId} exited ${result.code}: ${result.stderr.trim()}`);
  }
  const payload = Schema.decodeUnknownSync(Schema.Struct({items: Schema.Array(FleetDetailSchema)}))(
    JSON.parse(result.stdout.trim()),
  );
  const fleet = payload.items[0];
  if (payload.items.length !== 1 || fleet?.id !== fleetId) throw new Error("fleet show returned a different identity");
  return fleet;
}

export async function expectStatus(
  env: Env,
  fleetId: string,
  expected: string | ReadonlyArray<string>,
): Promise<typeof FleetDetailSchema.Type> {
  const payload = await getStatus(env, fleetId);
  const actual = payload.status;
  const allowed: ReadonlyArray<string> = Array.isArray(expected) ? expected : [expected as string];
  if (actual === undefined || !allowed.includes(actual)) {
    throw new Error(`expected status ${allowed.join("|")}, got ${actual} for ${fleetId}`);
  }
  return payload;
}
