//! SQL statement text for the fleets handler domain (RULE SQLMOD — new
//! modules keep query text here, grepable in one place; siblings with
//! pre-existing inline SQL keep their shape until extracted).
//!
//! `events_processed` and `budget_used_nanos` come from an indexed one-to-one
//! join to `core.fleet_activity_counters`, maintained by migration-030 triggers.
//! Neither the list nor the detail re-aggregates child tables per read.

/// The single-fleet detail read.
pub const SELECT_FLEET_DETAIL =
    \\SELECT f.id::text, f.name, f.status, f.source_markdown, f.trigger_markdown,
    \\       f.bundle_content_hash,
    \\       (f.config_json->'x-agentsfleet'->'triggers')::text,
    \\       COALESCE(c.events_processed, 0), COALESCE(c.budget_used_nanos, 0),
    \\       f.created_at, f.updated_at
    \\FROM core.fleets f
    \\LEFT JOIN core.fleet_activity_counters c ON c.fleet_id = f.id
    \\WHERE f.id = $1::uuid AND f.workspace_id = $2::uuid
;

// ── List page ────────────────────────────────────────────────────────────────
// A keyset-paged select with a one-to-one counter join: there is no per-row
// subselect and no child-table scan. `$1` workspace_id,
// `$2` limit for the first page; the after-cursor variant adds `$2` created_at
// + `$3` id and shifts limit to `$4`.

const PAGE_COLS =
    \\SELECT f.id::text, f.name, f.status, f.created_at, f.updated_at,
    \\       (f.config_json->'x-agentsfleet'->'triggers')::text,
    \\       COALESCE(c.events_processed, 0), COALESCE(c.budget_used_nanos, 0)
    \\FROM core.fleets f
    \\LEFT JOIN core.fleet_activity_counters c ON c.fleet_id = f.id
    \\
;

/// First page — the keyset's opening window.
pub const SELECT_FLEET_PAGE_FIRST = PAGE_COLS ++
    \\WHERE f.workspace_id = $1::uuid
    \\ORDER BY f.created_at DESC, f.id DESC
    \\LIMIT $2
;

/// Subsequent pages — keyset cursor on `(created_at, id)`.
pub const SELECT_FLEET_PAGE_AFTER = PAGE_COLS ++
    \\WHERE f.workspace_id = $1::uuid
    \\  AND (f.created_at < $2 OR (f.created_at = $2 AND f.id::text < $3))
    \\ORDER BY f.created_at DESC, f.id DESC
    \\LIMIT $4
;
