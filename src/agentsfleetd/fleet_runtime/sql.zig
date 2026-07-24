//! SQL statement text for the approval-gate domain (RULE SQLMOD — query text
//! lives here, grepable in one place).
//!
//! Every read is workspace- or action-scoped. The `($n::text = '' OR …)`
//! predicates are optional-filter idiom, not dead branches: one statement
//! serves both the fleet-scoped and workspace-wide callers, so the two can
//! never drift apart.

/// Gates past their deadline, oldest first. Bounded so a sweep cycle costs the
/// batch rather than the backlog; `idx_fleet_approval_gates_timeout_at_pending`
/// is partial on the pending status, which keeps the index small.
pub const SELECT_TIMED_OUT_GATES =
    \\SELECT action_id FROM core.fleet_approval_gates
    \\WHERE status = $1 AND timeout_at <= $2
    \\ORDER BY timeout_at ASC
    \\LIMIT $3
;

/// Resolve a gate, returning the row it settled.
///
/// The trailing `status = $6` is the race guard: only a gate still in the
/// expected state transitions, so two resolvers cannot both succeed and a
/// timeout cannot overwrite a human decision that landed first.
pub const RESOLVE_GATE =
    \\UPDATE core.fleet_approval_gates
    \\SET status = $1, detail = $2, resolved_by = $3, updated_at = $4
    \\WHERE action_id = $5 AND status = $6
    \\  AND ($7::text = '' OR fleet_id::text = $7)
    \\RETURNING id::text, action_id, workspace_id::text, fleet_id::text,
    \\          status, COALESCE(updated_at, $4::bigint), resolved_by, detail
;

/// The current gate for an action — newest wins, since an action may be gated
/// more than once over its life.
pub const SELECT_GATE_BY_ACTION =
    \\SELECT id::text, action_id, workspace_id::text, fleet_id::text,
    \\       status, COALESCE(updated_at, requested_at), resolved_by, detail
    \\FROM core.fleet_approval_gates
    \\WHERE action_id = $1
    \\  AND ($2::text = '' OR fleet_id::text = $2)
    \\ORDER BY requested_at DESC LIMIT 1
;

pub const SELECT_GATE_STATUS =
    \\SELECT status FROM core.fleet_approval_gates
    \\WHERE action_id = $1
    \\ORDER BY requested_at DESC LIMIT 1
;

pub const INSERT_GATE =
    \\INSERT INTO core.fleet_approval_gates
    \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
    \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
    \\   resolved_by, status, detail, requested_at, created_at)
    \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, $10, $11, '', $12, '', $13, $13)
;

/// One keyset page of pending gates for a workspace. The cursor compares the
/// `(requested_at, id)` pair as a tuple, so a page boundary falling inside a
/// group of same-instant rows neither repeats nor skips one.
pub const SELECT_GATE_PAGE =
    \\SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
    \\       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
    \\       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
    \\       g.status, g.detail, g.requested_at, g.timeout_at,
    \\       g.updated_at, g.resolved_by
    \\FROM core.fleet_approval_gates g
    \\JOIN core.fleets z ON z.id = g.fleet_id
    \\WHERE g.workspace_id = $1::uuid
    \\  AND g.status = $2
    \\  AND ($3 = '' OR g.fleet_id = $3::uuid)
    \\  AND ($4 = '' OR g.gate_kind = $4)
    \\  AND ($5 = false OR (g.requested_at, g.id::text) > ($6, $7))
    \\ORDER BY g.requested_at ASC, g.id ASC
    \\LIMIT $8
;

/// One gate by id, workspace-scoped so a valid id from another tenant misses.
pub const SELECT_GATE_BY_ID =
    \\SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
    \\       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
    \\       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
    \\       g.status, g.detail, g.requested_at, g.timeout_at,
    \\       g.updated_at, g.resolved_by
    \\FROM core.fleet_approval_gates g
    \\JOIN core.fleets z ON z.id = g.fleet_id
    \\WHERE g.id = $1::uuid AND g.workspace_id = $2::uuid
;
