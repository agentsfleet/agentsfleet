// Single-sourced PostHog product-event catalog. Event names and per-event prop
// shapes live here and nowhere else — call sites import EVENTS and
// captureProductEvent and never re-spell an event name. captureProductEvent
// takes only an EventName, so a misspelt name fails the typecheck.
//
// Props discipline: IDs, names, and enum values only. Never a token, raw API
// key, credential payload, or free-text typed into a sensitive field.

export const EVENTS = {
  workspace_switched: "workspace_switched",
  api_key_minted: "api_key_minted",
  model_added: "model_added",
  model_changed: "model_changed",
  key_rotated: "key_rotated",
  provider_reset: "provider_reset",
  secret_added: "secret_added",
  fleet_viewed: "fleet_viewed",
  // An operator forgot a memory entry from the console right rail.
  // Props carry the fleet id and outcome — never the memory key or content.
  fleet_memory_forgotten: "fleet_memory_forgotten",
  integration_requested: "integration_requested",
  approval_resolved: "approval_resolved",
  // The dashboard error boundary rendered. Until this existed the boundary
  // received the Error and dropped it, so every dashboard failure was invisible
  // — and the page now tells the user a support agent is working on it, which
  // is only true if the failure actually reaches somewhere a human reads.
  dashboard_error_shown: "dashboard_error_shown",
} as const;

export type EventName = (typeof EVENTS)[keyof typeof EVENTS];

export type EventProps = {
  [EVENTS.workspace_switched]: { workspace_id: string };
  [EVENTS.api_key_minted]: { api_key_id: string };
  [EVENTS.model_added]: { provider: string; mode: string; model?: string };
  [EVENTS.model_changed]: { provider: string; model: string };
  [EVENTS.key_rotated]: { provider: string };
  [EVENTS.provider_reset]: { from_provider: string };
  [EVENTS.secret_added]: { secret_name: string };
  [EVENTS.fleet_viewed]: { fleet_id: string; status: string };
  // Forgetting a memory is the operator's correction path. `outcome` is the
  // coarse success/failure. No memory `content`, no key text.
  [EVENTS.fleet_memory_forgotten]: { fleet_id: string; outcome: string };
  [EVENTS.integration_requested]: { integration_id: string; integration_name: string };
  [EVENTS.approval_resolved]: { gate_id: string; decision: string; has_reason: boolean };
  // `error_name` is the error's CLASS ("TypeError"), never its message: a
  // message is free text and routinely carries a URL, id, or payload fragment
  // from wherever it was thrown. `digest` is Next's server-error hash — in
  // production the real message is redacted and this is the only handle that
  // correlates the browser event with the server log. `attempt` distinguishes
  // a first failure from one that survived the automatic retries.
  [EVENTS.dashboard_error_shown]: {
    error_name: string;
    digest?: string;
    attempt: number;
  };
};

// Runtime mirror of EventProps — `satisfies` locks every array to that event's
// real prop keys, and the PII + emit-path tests assert against it (the type
// alone is erased at runtime).
export const EVENT_PROP_KEYS = {
  [EVENTS.workspace_switched]: ["workspace_id"],
  [EVENTS.api_key_minted]: ["api_key_id"],
  [EVENTS.model_added]: ["provider", "mode", "model"],
  [EVENTS.model_changed]: ["provider", "model"],
  [EVENTS.key_rotated]: ["provider"],
  [EVENTS.provider_reset]: ["from_provider"],
  [EVENTS.secret_added]: ["secret_name"],
  [EVENTS.fleet_viewed]: ["fleet_id", "status"],
  [EVENTS.fleet_memory_forgotten]: ["fleet_id", "outcome"],
  [EVENTS.integration_requested]: ["integration_id", "integration_name"],
  [EVENTS.approval_resolved]: ["gate_id", "decision", "has_reason"],
  [EVENTS.dashboard_error_shown]: ["error_name", "digest", "attempt"],
} as const satisfies { [E in EventName]: readonly (keyof EventProps[E])[] };
