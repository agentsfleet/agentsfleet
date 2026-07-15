// Single-sourced PostHog product-event catalog. Event names and per-event prop
// shapes live here and nowhere else — call sites import EVENTS and
// captureProductEvent and never re-spell an event name (the analytics-events
// grep test fails on drift).
//
// Props discipline: IDs, names, and enum values only. Never a token, raw API
// key, credential payload, or free-text typed into a sensitive field.

export const EVENTS = {
  fleet_created: "fleet_created",
  workspace_switched: "workspace_switched",
  runner_token_minted: "runner_token_minted",
  api_key_minted: "api_key_minted",
  model_added: "model_added",
  model_changed: "model_changed",
  key_rotated: "key_rotated",
  provider_reset: "provider_reset",
  platform_default_set: "platform_default_set",
  secret_added: "secret_added",
  fleet_library_onboarded: "fleet_library_onboarded",
  platform_library_onboarded: "platform_library_onboarded",
  platform_library_published: "platform_library_published",
  platform_library_source_changed: "platform_library_source_changed",
  fleet_viewed: "fleet_viewed",
  // An operator saved an edited SKILL.md / TRIGGER.md from the console left rail
  // (M131 §4). Props carry the fleet id, which half was saved, and the coarse
  // outcome — never the source contents.
  fleet_source_saved: "fleet_source_saved",
  // An operator forgot a memory entry from the console right rail (M131 §5).
  // Props carry the fleet id and outcome — never the memory key or content.
  fleet_memory_forgotten: "fleet_memory_forgotten",
  integration_requested: "integration_requested",
  approval_resolved: "approval_resolved",
} as const;

export type EventName = (typeof EVENTS)[keyof typeof EVENTS];

export type EventProps = {
  [EVENTS.fleet_created]: { fleet_id: string };
  [EVENTS.workspace_switched]: { workspace_id: string };
  [EVENTS.runner_token_minted]: { runner_id: string; sandbox_tier: string };
  [EVENTS.api_key_minted]: { api_key_id: string };
  [EVENTS.model_added]: { provider: string; mode: string; model?: string };
  [EVENTS.model_changed]: { provider: string; model: string };
  [EVENTS.key_rotated]: { provider: string };
  [EVENTS.provider_reset]: { from_provider: string };
  [EVENTS.platform_default_set]: { provider: string; model: string; is_custom: boolean };
  [EVENTS.secret_added]: { secret_name: string };
  [EVENTS.fleet_library_onboarded]: {
    workspace_id: string;
    visibility: string;
    source_kind: string;
    outcome: string;
  };
  // Platform-tier onboarding. No workspace_id — the platform catalog has no
  // workspace segment. `entry_id` is the catalog slug the importer derived from
  // the bundle (e.g. "platform-ops"), present only when the onboard succeeded;
  // the repository the operator typed is never sent as free text.
  [EVENTS.platform_library_onboarded]: {
    source_kind: string;
    outcome: string;
    entry_id?: string;
  };
  // Publishing is the moment a fleet becomes available to EVERY tenant — the one
  // state change on the catalog surface with a decision riding on it. `entry_id`
  // is the catalog slug the importer derived, never operator free text.
  [EVENTS.platform_library_published]: {
    entry_id: string;
    action: string;
    outcome: string;
  };
  // Repointing a catalog entry's source is the one operator edit that silently
  // withdraws a fleet from every workspace gallery (the server discards the
  // bundle and drafts the row). `field` names which half moved (repo|ref);
  // `was_published` says whether a live fleet was withdrawn by it.
  [EVENTS.platform_library_source_changed]: {
    entry_id: string;
    field: string;
    was_published: boolean;
    outcome: string;
  };
  [EVENTS.fleet_viewed]: { fleet_id: string; status: string };
  // Saving the source is the one console edit that changes what the fleet does
  // on its next wake. `field` names which half moved (skill|trigger); `outcome`
  // is the coarse success/failure. No source markdown, no credential material.
  [EVENTS.fleet_source_saved]: { fleet_id: string; field: string; outcome: string };
  // Forgetting a memory is the operator's correction path. `outcome` is the
  // coarse success/failure. No memory `content`, no key text.
  [EVENTS.fleet_memory_forgotten]: { fleet_id: string; outcome: string };
  [EVENTS.integration_requested]: { integration_id: string; integration_name: string };
  [EVENTS.approval_resolved]: { gate_id: string; decision: string; has_reason: boolean };
};

// Runtime mirror of EventProps — `satisfies` locks every array to that event's
// real prop keys, and the PII + emit-path tests assert against it (the type
// alone is erased at runtime).
export const EVENT_PROP_KEYS = {
  [EVENTS.fleet_created]: ["fleet_id"],
  [EVENTS.workspace_switched]: ["workspace_id"],
  [EVENTS.runner_token_minted]: ["runner_id", "sandbox_tier"],
  [EVENTS.api_key_minted]: ["api_key_id"],
  [EVENTS.model_added]: ["provider", "mode", "model"],
  [EVENTS.model_changed]: ["provider", "model"],
  [EVENTS.key_rotated]: ["provider"],
  [EVENTS.provider_reset]: ["from_provider"],
  [EVENTS.platform_default_set]: ["provider", "model", "is_custom"],
  [EVENTS.secret_added]: ["secret_name"],
  [EVENTS.fleet_library_onboarded]: ["workspace_id", "visibility", "source_kind", "outcome"],
  [EVENTS.platform_library_onboarded]: ["source_kind", "outcome", "entry_id"],
  [EVENTS.platform_library_published]: ["entry_id", "action", "outcome"],
  [EVENTS.platform_library_source_changed]: ["entry_id", "field", "was_published", "outcome"],
  [EVENTS.fleet_viewed]: ["fleet_id", "status"],
  [EVENTS.fleet_source_saved]: ["fleet_id", "field", "outcome"],
  [EVENTS.fleet_memory_forgotten]: ["fleet_id", "outcome"],
  [EVENTS.integration_requested]: ["integration_id", "integration_name"],
  [EVENTS.approval_resolved]: ["gate_id", "decision", "has_reason"],
} as const satisfies { [E in EventName]: readonly (keyof EventProps[E])[] };
