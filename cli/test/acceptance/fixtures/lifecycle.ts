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

// An archived fleet refuses every further transition: `transition` in
// `afd_authoring/src/creation/patch.rs` answers it as a missing fleet
// (UZ-AGT-009, a 404). Delete has no status precondition, so it has no
// refusal of its own to classify.
//
// The pattern carries the sentence alongside its code because that is the
// half a person reads, and it is the half that moved: the CLI now renders the
// daemon's `user_message` ("We couldn't find that Fleet") where it used to
// print the log-side `detail` ("fleet not found"). A regex that knew only the
// old wording went red on a rendering change and named nothing about the
// product.

/** An archived fleet refuses every further transition, as a tombstone. */
export const TOMBSTONE_REFUSAL =
  /UZ-AGT-009|couldn't find that Fleet|HTTP_404|Not Found/i;

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
