// Named check identifiers emitted by `agentsfleet doctor`. The strings
// appear in the JSON output and in stdout summaries; downstream tools
// can grep on them. Stable surface — treat like the analytics events.
//
// RULE UFS.

export const DOCTOR_CHECK = Object.freeze({
  SERVER_REACHABLE: "server_reachable",
  WORKSPACE_SELECTED: "workspace_selected",
  WORKSPACE_BINDING_VALID: "workspace_binding_valid",
});
