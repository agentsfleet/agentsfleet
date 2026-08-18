// The runner admin-action vocabulary: confirm-dialog copy, eligibility rules
// and glyphs, shared by the detail header (and formerly the table rows the
// wall replaced). Presentation-only; the callers own state and data flow.

import {
  RUNNER_ADMIN_ACTION,
  RUNNER_ADMIN_STATE,
  type RunnerStateAction,
  type RunnerAdminState,
} from "@/lib/api/runners";

// Each admin action carries its confirm-dialog copy. `label` is the single
// source of the accessible name; revoke is offered until the runner is
// revoked, delete only after (DELETE /v1/fleets/runners/{id} 409s on a live
// runner), exactly as ApiKeyList alternates the two.
export const ACTION_CONFIG: Record<RunnerStateAction, {
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
// RunnerStateAction, the PATCH verbs that MOVE admin_state, and widening it
// would loosen an exhaustive type that actionsFor and RunnerHeader lean on.
// Delete is a different HTTP verb with a different lifecycle, so it gets its own
// config and its own trigger. `self_test` is out for the same reason from the
// other direction: it is a PATCH verb, but it records a request and transitions
// nothing, so it carries its own config below.
export const DELETE_ACTION_CONFIG = {
  label: "Delete",
  title: "Delete this runner?",
  description:
    "Removes the revoked runner's record, along with its lease and event history. The enrolled host is unaffected — it was already blocked at revoke. This cannot be undone.",
  intent: "destructive" as const,
  errorAction: "delete this runner",
};

// The self-test trigger. No confirm dialog: it runs a read-only probe inside the
// runner's own sandbox and changes nothing an operator would want to undo, so a
// confirmation would be ceremony without a decision behind it.
export const SELFTEST_ACTION_CONFIG = {
  label: "Run self-test",
  pendingLabel: "Self-test requested",
  errorAction: "run a self-test on this runner",
};

/** Only a revoked runner is deletable — the daemon 409s (UZ-RUN-016) otherwise. */
export function canDelete(state: RunnerAdminState): boolean {
  return state === RUNNER_ADMIN_STATE.revoked;
}

/** A revoked runner never heartbeats again, so it can never answer a self-test
 * request — the daemon refuses it (UZ-RUN-018) and the control does not render. */
export function canSelftest(state: RunnerAdminState): boolean {
  return state !== RUNNER_ADMIN_STATE.revoked;
}

export function actionsFor(state: RunnerAdminState): RunnerStateAction[] {
  const out: RunnerStateAction[] = [];
  if (state === RUNNER_ADMIN_STATE.active) out.push(RUNNER_ADMIN_ACTION.cordon);
  if (state === RUNNER_ADMIN_STATE.active || state === RUNNER_ADMIN_STATE.cordoned) out.push(RUNNER_ADMIN_ACTION.drain);
  if (state !== RUNNER_ADMIN_STATE.revoked) out.push(RUNNER_ADMIN_ACTION.revoke);
  return out;
}
