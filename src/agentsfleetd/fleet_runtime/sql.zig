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
/// Resolve a gate and, when it is an integration-grant gate, move the standing
/// grant in the SAME statement.
///
/// The grant arm rides here rather than following as a second statement for the
/// reason the lifetime tally rides the lease status flip: a standing
/// authorization that can outlive its decision by a failed follow-up write is a
/// grant nobody approved. One statement, one commit, no drift.
///
/// `$8` is the gate's approved verdict — compared against the NEW status, so the
/// arm fires only on approval. Anything else (denied, timed out, auto-killed)
/// drives the grant to `$10` rather than leaving it `pending`, which nothing
/// would ever re-raise. `$11` selects the kind; gates of any other kind leave
/// `core.integration_grants` untouched because no row matches.
///
/// `g.status != $10` (not revoked) closes a resurrection hole: an explicit
/// revoke (the standalone DELETE endpoint) never touches the gate row, so a
/// gate raised before the revoke and left unresolved would otherwise arm the
/// grant right back on its eventual approval — silently reversing a decision
/// this statement had no part in and no visibility into.
pub const RESOLVE_GATE =
    \\WITH resolved AS (
    \\  UPDATE core.fleet_approval_gates
    \\  SET status = $1, detail = $2, resolved_by = $3, updated_at = $4
    \\  WHERE action_id = $5 AND status = $6
    \\    AND ($7::text = '' OR fleet_id::text = $7)
    \\  RETURNING id, action_id, workspace_id, fleet_id, status,
    \\            updated_at, resolved_by, detail, gate_kind, evidence
    \\), granted AS (
    \\  UPDATE core.integration_grants g
    \\  SET status      = CASE WHEN r.status = $8 THEN $9 ELSE $10 END,
    \\      approved_at = CASE WHEN r.status = $8 THEN $4 END,
    \\      revoked_at  = CASE WHEN r.status = $8 THEN NULL ELSE $4 END
    \\  FROM resolved r
    \\  WHERE g.fleet_id = r.fleet_id
    \\    AND g.service  = r.evidence->>'service'
    \\    AND r.gate_kind = $11
    \\    AND g.status != $10
    \\  RETURNING g.id
    \\)
    \\SELECT id::text, action_id, workspace_id::text, fleet_id::text,
    \\       status, COALESCE(updated_at, $4::bigint), resolved_by, detail
    \\FROM resolved
;

/// The current gate for an action — newest wins, since an action may be gated
/// more than once over its life.
pub const SELECT_GATE_BY_ACTION =
    \\SELECT id::text, action_id, workspace_id::text, fleet_id::text,
    \\       status, COALESCE(updated_at, created_at), resolved_by, detail
    \\FROM core.fleet_approval_gates
    \\WHERE action_id = $1
    \\  AND ($2::text = '' OR fleet_id::text = $2)
    \\ORDER BY created_at DESC LIMIT 1
;

pub const SELECT_GATE_STATUS =
    \\SELECT status FROM core.fleet_approval_gates
    \\WHERE action_id = $1
    \\ORDER BY created_at DESC LIMIT 1
;

/// The approved write gate whose compact identifier becomes the daemon-authored
/// repair branch. The caller performs semantic binding equality because
/// repository case and order are intentionally insignificant.
pub const SELECT_APPROVED_WRITE_GATE_ID =
    \\SELECT id::text, stated_binding::text FROM core.fleet_approval_gates
    \\WHERE fleet_id = $1::uuid AND event_id = $2 AND gate_kind = $3
    \\  AND status = $4 AND updated_at IS NOT NULL AND updated_at <= timeout_at
    \\  AND stated_binding IS NOT NULL
    \\  AND spend_count IS NOT NULL AND spend_ceiling = $5
    \\ORDER BY created_at DESC, id DESC
    \\LIMIT 1
;

pub const INSERT_GATE =
    \\INSERT INTO core.fleet_approval_gates
    \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name,
    \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
    \\   resolved_by, status, detail, created_at, event_id, stated_binding,
    \\   spend_count, spend_ceiling)
    \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, $10, $11, '', $12, '', $13, $14, $15::jsonb, $16, $17)
;

/// One keyset page of pending gates for a workspace. The cursor compares the
/// `(created_at, id)` pair as a tuple, so a page boundary falling inside a
/// group of same-instant rows neither repeats nor skips one.
pub const SELECT_GATE_PAGE =
    \\SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
    \\       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
    \\       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
    \\       g.status, g.detail, g.created_at, g.timeout_at,
    \\       g.updated_at, g.resolved_by
    \\FROM core.fleet_approval_gates g
    \\JOIN core.fleets z ON z.id = g.fleet_id
    \\WHERE g.workspace_id = $1::uuid
    \\  AND g.status = $2
    \\  AND ($3 = '' OR g.fleet_id = $3::uuid)
    \\  AND ($4 = '' OR g.gate_kind = $4)
    \\  AND ($5 = false OR (g.created_at, g.id::text) > ($6, $7))
    \\ORDER BY g.created_at ASC, g.id ASC
    \\LIMIT $8
;

/// One gate by id, workspace-scoped so a valid id from another tenant misses.
pub const SELECT_GATE_BY_ID =
    \\SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
    \\       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
    \\       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
    \\       g.status, g.detail, g.created_at, g.timeout_at,
    \\       g.updated_at, g.resolved_by
    \\FROM core.fleet_approval_gates g
    \\JOIN core.fleets z ON z.id = g.fleet_id
    \\WHERE g.id = $1::uuid AND g.workspace_id = $2::uuid
;
