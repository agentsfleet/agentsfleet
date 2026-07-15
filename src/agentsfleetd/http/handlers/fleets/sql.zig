//! SQL statement text for the fleets handler domain (RULE SQLMOD — new
//! modules keep query text here, grepable in one place; siblings with
//! pre-existing inline SQL keep their shape until extracted).
//!
//! `events_processed` and `budget_used_nanos` are read straight off
//! `core.fleets` — denormalized counters maintained by the migration-030
//! triggers, so neither the list nor the detail re-aggregates the child tables
//! per read (the Live Wall is the hot path; see migration 030's header for the
//! why and the measured numbers).

/// The single-fleet detail read.
pub const SELECT_FLEET_DETAIL =
    \\SELECT id::text, name, status, source_markdown, trigger_markdown,
    \\       bundle_content_hash,
    \\       (config_json->'x-agentsfleet'->'triggers')::text,
    \\       events_processed, budget_used_nanos,
    \\       created_at, updated_at
    \\FROM core.fleets
    \\WHERE id = $1::uuid AND workspace_id = $2::uuid
;

// ── List page ────────────────────────────────────────────────────────────────
// A plain keyset-paged select of the fleet rows: the aggregates are columns, so
// there is no per-row subselect and no child-table scan. `$1` workspace_id,
// `$2` limit for the first page; the after-cursor variant adds `$2` created_at
// + `$3` id and shifts limit to `$4`.

const PAGE_COLS =
    \\SELECT id::text, name, status, created_at, updated_at,
    \\       (config_json->'x-agentsfleet'->'triggers')::text,
    \\       events_processed, budget_used_nanos
    \\FROM core.fleets
    \\
;

/// First page — the keyset's opening window.
pub const SELECT_FLEET_PAGE_FIRST = PAGE_COLS ++
    \\WHERE workspace_id = $1::uuid
    \\ORDER BY created_at DESC, id DESC
    \\LIMIT $2
;

/// Subsequent pages — keyset cursor on `(created_at, id)`.
pub const SELECT_FLEET_PAGE_AFTER = PAGE_COLS ++
    \\WHERE workspace_id = $1::uuid
    \\  AND (created_at < $2 OR (created_at = $2 AND id::text < $3))
    \\ORDER BY created_at DESC, id DESC
    \\LIMIT $4
;
