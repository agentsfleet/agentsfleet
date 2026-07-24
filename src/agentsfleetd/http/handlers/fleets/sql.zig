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

// ── Fleet lifecycle ─────────────────────────────────────────────────────────
// Every statement is workspace-scoped in its predicate, so a valid fleet id
// belonging to another workspace resolves nothing rather than the wrong row.
// That is the tenancy boundary, enforced in SQL rather than only in the handler.

pub const INSERT_FLEET =
    \\INSERT INTO core.fleets
    \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
    \\   status, required_tags, bundle_content_hash,
    \\   bundle_snapshot_key, created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6::jsonb, $7, $8::text[],
    \\        $9, $10, $11, $11)
;

/// Status flip, guarded on the expected current status so a concurrent
/// transition cannot be silently overwritten.
pub const UPDATE_FLEET_STATUS =
    \\UPDATE core.fleets SET status = $1, updated_at = $2
    \\WHERE id = $3::uuid AND workspace_id = $4::uuid AND status = $5
;

pub const DELETE_FLEET =
    \\DELETE FROM core.fleets WHERE id = $1::uuid AND workspace_id = $2::uuid
;

pub const SELECT_FLEET_CONFIG_AND_STATUS =
    \\SELECT config_json::text, status
    \\FROM core.fleets
    \\WHERE id = $1::uuid AND workspace_id = $2::uuid
;

pub const SELECT_FLEET_STATUS =
    \\SELECT status FROM core.fleets
    \\WHERE id = $1::uuid AND workspace_id = $2::uuid
    \\LIMIT 1
;

/// Delete only from the expected status, reporting whether it happened.
/// `RETURNING id` is what lets the caller distinguish "already gone" from
/// "still running, refused" without a second read.
pub const DELETE_FLEET_IN_STATUS =
    \\DELETE FROM core.fleets
    \\WHERE id = $1::uuid AND workspace_id = $2::uuid AND status = $3
    \\RETURNING id
;

/// Lock a fleet for the read-modify-write of a PATCH.
pub const SELECT_FLEET_FOR_UPDATE =
    \\SELECT name, status, source_markdown, trigger_markdown FROM core.fleets
    \\WHERE id = $1::uuid AND workspace_id = $2::uuid
    \\FOR UPDATE
;

/// Apply a PATCH. `COALESCE` per column makes every field independently
/// optional, so an absent field is untouched rather than nulled.
///
/// The trailing disjunction is the state machine, expressed in SQL: a status
/// change is accepted only when it is a no-op, or a transition whose source
/// status is in the allowed set for that target. An illegal transition matches
/// no row and returns nothing, so the handler cannot be talked into one.
pub const PATCH_FLEET =
    \\UPDATE core.fleets SET
    \\    config_json      = COALESCE($1::jsonb, config_json),
    \\    status           = COALESCE($2,        status),
    \\    trigger_markdown = COALESCE($11,       trigger_markdown),
    \\    source_markdown  = COALESCE($12,       source_markdown),
    \\    name             = COALESCE($13,       name),
    \\    required_tags    = COALESCE($14::text[], required_tags),
    \\    updated_at       = $3
    \\WHERE id = $4::uuid
    \\  AND workspace_id = $5::uuid
    \\  AND status != $6
    \\  AND (
    \\        $2::text IS NULL
    \\     OR ($2 = $6)
    \\     OR ($2 = $7 AND status = ANY($9::text[]))
    \\     OR ($2 = $8 AND status = ANY($10::text[]))
    \\  )
    \\RETURNING updated_at
;
