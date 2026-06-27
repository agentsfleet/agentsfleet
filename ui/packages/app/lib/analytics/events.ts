// Single-sourced PostHog product-event catalog. Event names and per-event prop
// shapes live here and nowhere else — call sites import EVENTS and
// captureProductEvent and never re-spell an event name (the analytics-events
// grep test fails on drift).
//
// Props discipline: IDs, names, and enum values only. Never a token, raw API
// key, credential payload, or free-text typed into a sensitive field.

export const EVENTS = {
  fleet_created: "fleet_created",
  runner_token_minted: "runner_token_minted",
  api_key_minted: "api_key_minted",
  model_added: "model_added",
  credential_added: "credential_added",
  integration_requested: "integration_requested",
  approval_resolved: "approval_resolved",
} as const;

export type EventName = (typeof EVENTS)[keyof typeof EVENTS];

export type EventProps = {
  [EVENTS.fleet_created]: { fleet_id: string };
  [EVENTS.runner_token_minted]: { runner_id: string; sandbox_tier: string };
  [EVENTS.api_key_minted]: { api_key_id: string };
  [EVENTS.model_added]: { provider: string; mode: string; model?: string };
  [EVENTS.credential_added]: { credential_name: string };
  [EVENTS.integration_requested]: { integration_id: string; integration_name: string };
  [EVENTS.approval_resolved]: { gate_id: string; decision: string; has_reason: boolean };
};

// Runtime mirror of EventProps — `satisfies` locks every array to that event's
// real prop keys, and the PII + emit-path tests assert against it (the type
// alone is erased at runtime).
export const EVENT_PROP_KEYS = {
  [EVENTS.fleet_created]: ["fleet_id"],
  [EVENTS.runner_token_minted]: ["runner_id", "sandbox_tier"],
  [EVENTS.api_key_minted]: ["api_key_id"],
  [EVENTS.model_added]: ["provider", "mode", "model"],
  [EVENTS.credential_added]: ["credential_name"],
  [EVENTS.integration_requested]: ["integration_id", "integration_name"],
  [EVENTS.approval_resolved]: ["gate_id", "decision", "has_reason"],
} as const satisfies { [E in EventName]: readonly (keyof EventProps[E])[] };
