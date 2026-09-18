// Which Fleets are waiting on a decision, and why a steer stopped producing
// output.
//
// A Fleet whose trigger declares a write capability opens an approval gate per
// run and waits. Before this lookup the only signal in the terminal was the
// sixty-second timeout, which reads as "the daemon is slow" and is not — the
// run is parked, deliberately, on a decision nobody has been asked to make.
//
// The lookup runs on the failure path only, so a healthy steer pays nothing for
// it, and a failed one spends one request to turn a dead end into a next step.

import { Effect, Exit, type Redacted } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { GATE_STATUS } from "../constants/approvals.ts";
import { fetchGates, type ApprovalGate } from "./approvals.ts";

const ONE_GATE = 1;

/** Every pending gate in the workspace, or an empty list when the inbox cannot
 *  be read.
 *
 *  The daemon does the narrowing: `status=pending` is its own query parameter,
 *  and the read pages to exhaustion. Filtering a single returned page here
 *  would report "nothing waiting" for a workspace whose pending gate sits
 *  behind a page of decided ones — a confident wrong answer, which is the
 *  failure this whole diagnosis replaces.
 *
 *  Best-effort at both call sites: a failed diagnosis must never replace the
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
): Effect.Effect<ReadonlyArray<ApprovalGate> | null, never, HttpClient> =>
  Effect.gen(function* () {
    const exit = yield* Effect.exit(
      fetchGates(wsId, token, { status: GATE_STATUS.pending }),
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

/** The sentence naming the gate, or null when nothing is holding this Fleet. */
export const parkedGateHint = (
  wsId: string,
  fleetId: string,
  token: Redacted.Redacted<string>,
): Effect.Effect<string | null, never, HttpClient> =>
  Effect.gen(function* () {
    const all = yield* pendingGates(wsId, token);
    // Unreadable inbox: the caller keeps the failure it already has and its
    // ordinary suggestion. Silence is honest here; "nothing is waiting" is not.
    if (all === null) return null;
    const pending = all.filter((gate) => gate.fleet_id === fleetId);
    const first = pending[0];
    if (first === undefined) return null;
    return pending.length === ONE_GATE
      ? `waiting on approval gate ${first.gate_id ?? ""} — decide it with: agentsfleet approvals approve ${first.gate_id ?? ""}`
      : `waiting on ${pending.length} approval gates — list them with: agentsfleet approvals list --fleet ${fleetId}`;
  });
