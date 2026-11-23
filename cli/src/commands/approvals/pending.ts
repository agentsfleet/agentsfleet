// Which Fleets are waiting on a decision.
//
// A Fleet whose trigger declares a write capability opens an approval gate per
// run and waits, deliberately, on a decision somebody has to make.

import { Effect, Exit, type Redacted } from "effect";
import { HttpClient } from "../../services/http-client.ts";
import { GATE_STATUS } from "../../constants/approvals.ts";
import { fetchGates, type ApprovalGate } from "./approvals.ts";

/** Every pending gate in the workspace, or an empty list when the inbox cannot
 *  be read.
 *
 *  The daemon does the narrowing: `status=pending` is its own query parameter,
 *  and the read pages to exhaustion. Filtering a single returned page here
 *  would report "nothing waiting" for a workspace whose pending gate sits
 *  behind a page of decided ones — a confident wrong answer, which is the
 *  failure this whole diagnosis replaces.
 *
 *  Best-effort: a failed diagnosis must never replace the
 *  thing being diagnosed, and `Effect.exit` survives a defect (a thrown
 *  TypeError in the transport) as well as a typed refusal.
 *
 *  `null` when the inbox could not be read, and that distinction is the point.
 *  Collapsing a failed read into an empty list renders `Waiting: 0` for a
 *  parked Fleet — which is the exact failure this whole diagnosis exists to
 *  remove, arriving through the error path instead of the happy one. Reading
 *  the inbox needs `approval:read`, which a credential holding `fleet:read`
 *  may not carry, so the failure is ordinary rather than exotic. */
const pendingGates = (
  wsId: string,
  token: Redacted.Redacted<string>,
  signal?: AbortSignal,
): Effect.Effect<ReadonlyArray<ApprovalGate> | null, never, HttpClient> =>
  Effect.gen(function* () {
    const exit = yield* Effect.exit(
      fetchGates(wsId, token, { status: GATE_STATUS.pending }, signal),
    );
    return Exit.isSuccess(exit) ? exit.value : null;
  });

/**
 * Pending-gate count per Fleet identifier.
 *
 * `status` reads this rather than a field on the Fleet row: the list endpoint
 * does not carry `pending_approvals` (only the per-Fleet detail does), so a
 * column sourced from the row would have printed 0 for a parked Fleet — a
 * confident wrong answer, which is worse than the silence it replaced.
 */
export const pendingGateCounts = (
  wsId: string,
  token: Redacted.Redacted<string>,
): Effect.Effect<ReadonlyMap<string, number> | null, never, HttpClient> =>
  Effect.gen(function* () {
    const gates = yield* pendingGates(wsId, token);
    if (gates === null) return null;
    const counts = new Map<string, number>();
    for (const gate of gates) {
      const fleetId = gate.fleet_id;
      if (!fleetId) continue;
      counts.set(fleetId, (counts.get(fleetId) ?? 0) + 1);
    }
    return counts;
  });
