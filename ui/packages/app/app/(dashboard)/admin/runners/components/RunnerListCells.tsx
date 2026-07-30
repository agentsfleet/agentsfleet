// The runner admin-action vocabulary: confirm-dialog copy, eligibility rules
// and glyphs, shared by the detail header (and formerly the table rows the
// wall replaced). Presentation-only; the callers own state and data flow.

import {
  RUNNER_ADMIN_ACTION,
  RUNNER_ADMIN_STATE,
  type RunnerAdminAction,
  type RunnerAdminState,
} from "@/lib/api/runners";

// Each admin action carries its confirm-dialog copy. `label` is the single
// source of the accessible name; revoke is offered until the runner is
// revoked, delete only after (DELETE /v1/fleets/runners/{id} 409s on a live
// runner), exactly as ApiKeyList alternates the two.
export const ACTION_CONFIG: Record<RunnerAdminAction, {
  label: string;
  title: string;
  description: string;
  intent: "default" | "destructive";
  errorAction: string;
}> = {
  [RUNNER_ADMIN_ACTION.cordon]: {
    label: "Cordon",
    title: "Cordon this runner?",
    description: "Runner-plane calls stop immediately. Existing lease rows stay fenced until expiry or reassignment.",
    intent: "default",
    errorAction: "cordon this runner",
  },
  [RUNNER_ADMIN_ACTION.drain]: {
    label: "Drain",
    title: "Drain this runner?",
    description: "The runner stops taking new work and becomes drained automatically once active leases reach zero.",
    intent: "default",
    errorAction: "drain this runner",
  },
  [RUNNER_ADMIN_ACTION.revoke]: {
    label: "Revoke",
    title: "Revoke this runner?",
    description: "The runner token is blocked immediately. This is terminal for the enrolled host.",
    intent: "destructive",
    errorAction: "revoke this runner",
  },
};

// Delete is deliberately NOT a member of ACTION_CONFIG: that map is keyed on
// RunnerAdminAction, the three PATCH verbs the daemon serves, and widening it
// would loosen an exhaustive type that actionsFor and RunnerHeader lean on.
// Delete is a different HTTP verb with a different lifecycle, so it gets its own
// config and its own trigger.
export const DELETE_ACTION_CONFIG = {
  label: "Delete",
  title: "Delete this runner?",
  description:
    "Removes the revoked runner's record, along with its lease and event history. The enrolled host is unaffected — it was already blocked at revoke. This cannot be undone.",
  intent: "destructive" as const,
  errorAction: "delete this runner",
};

/** Only a revoked runner is deletable — the daemon 409s (UZ-RUN-016) otherwise. */
export function canDelete(state: RunnerAdminState): boolean {
  return state === RUNNER_ADMIN_STATE.revoked;
}

export function actionsFor(state: RunnerAdminState): RunnerAdminAction[] {
  const out: RunnerAdminAction[] = [];
  if (state === RUNNER_ADMIN_STATE.active) out.push(RUNNER_ADMIN_ACTION.cordon);
  if (state === RUNNER_ADMIN_STATE.active || state === RUNNER_ADMIN_STATE.cordoned) out.push(RUNNER_ADMIN_ACTION.drain);
  if (state !== RUNNER_ADMIN_STATE.revoked) out.push(RUNNER_ADMIN_ACTION.revoke);
  return out;
}
