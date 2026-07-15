//! Centralized SQL for the workspace onboarding signal read.

// One round trip for every derivable onboarding signal. Each is an EXISTS
// subquery — the planner stops at the first matching row, so none of these
// scans a table. The workspace signals key on the workspace_id index; the
// tenant-model check keys on tenant_model_selection's tenant_id primary key.
// The steer prefix is bound as a parameter ($2), never inlined, per RULE NSQ.
pub const SELECT_SIGNALS =
    \\SELECT
    \\  EXISTS(SELECT 1 FROM core.fleets WHERE workspace_id = $1::uuid)                              AS has_fleet,
    \\  EXISTS(SELECT 1 FROM vault.secrets WHERE workspace_id = $1::uuid)                            AS has_secret,
    \\  EXISTS(SELECT 1 FROM core.fleet_events WHERE workspace_id = $1::uuid)                        AS has_event,
    \\  EXISTS(SELECT 1 FROM core.fleet_events WHERE workspace_id = $1::uuid AND actor LIKE $2)      AS has_steer,
    \\  EXISTS(SELECT 1 FROM core.tenant_model_selection
    \\         WHERE tenant_id = $3::uuid AND length(btrim(model)) > 0)                              AS tenant_model
;
