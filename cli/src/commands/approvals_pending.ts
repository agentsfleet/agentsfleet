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
import { wsApprovalsPath } from "../lib/api-paths.ts";
import { GATE_STATUS } from "../constants/approvals.ts";
import type { ApprovalGate } from "./approvals.ts";

interface ApprovalListResponse {
  readonly items?: ReadonlyArray<ApprovalGate>;
}

const ONE_GATE = 1;

/** Every pending gate in the workspace, or an empty list when the inbox cannot
 *  be read. A failed diagnosis must never replace the thing being diagnosed,
 *  so the read is best-effort at both call sites. */
const pendingGates = (
  wsId: string,
  token: Redacted.Redacted<string>,
): Effect.Effect<ReadonlyArray<ApprovalGate>, never, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    // `Effect.exit` rather than an error-channel fallback: this runs on a path
    // that has ALREADY failed, so it must survive a defect (a thrown TypeError
    // in the transport, say) as well as a typed refusal. A diagnosis that can
    // itself crash is worse than no diagnosis.
    const exit = yield* Effect.exit(
      http.request<ApprovalListResponse>({ path: wsApprovalsPath(wsId), token }),
    );
    const res = Exit.isSuccess(exit) ? exit.value : ({} as ApprovalListResponse);
    return (res.items ?? []).filter(
      (gate) => gate.status === GATE_STATUS.pending,
    );
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
): Effect.Effect<ReadonlyMap<string, number>, never, HttpClient> =>
  Effect.gen(function* () {
    const gates = yield* pendingGates(wsId, token);
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
    const pending = all.filter((gate) => gate.fleet_id === fleetId);
    const first = pending[0];
    if (first === undefined) return null;
    return pending.length === ONE_GATE
      ? `waiting on approval gate ${first.gate_id ?? ""} — decide it with: agentsfleet approvals approve ${first.gate_id ?? ""}`
      : `waiting on ${pending.length} approval gates — list them with: agentsfleet approvals list --fleet ${fleetId}`;
  });
