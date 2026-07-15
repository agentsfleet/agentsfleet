//! SQL statement text for the fleets handler domain (RULE SQLMOD — new
//! modules keep query text here, grepable in one place; siblings with
//! pre-existing inline SQL keep their shape until extracted).

/// The single-fleet detail read. Reads ONE row, so the two aggregates stay
/// correlated subselects — a `GROUP BY` join buys nothing at cardinality 1,
/// and this keeps the detail projection legible. `fleet_execution_telemetry
/// .fleet_id` is TEXT, hence the `id::text` join key.
pub const SELECT_FLEET_DETAIL =
    \\SELECT id::text, name, status, source_markdown, trigger_markdown,
    \\       bundle_content_hash,
    \\       (config_json->'x-agentsfleet'->'triggers')::text,
    \\       (SELECT COUNT(*) FROM core.fleet_events ev WHERE ev.fleet_id = core.fleets.id)::bigint,
    \\       (SELECT COALESCE(SUM(te.credit_deducted_nanos), 0)::bigint
    \\          FROM core.fleet_execution_telemetry te WHERE te.fleet_id = core.fleets.id::text),
    \\       created_at, updated_at
    \\FROM core.fleets
    \\WHERE id = $1::uuid AND workspace_id = $2::uuid
;

// ── List page: single-pass aggregates ───────────────────────────────────────
// The old shape ran BOTH aggregates as correlated subselects, once per fleet
// row — up to 200 subquery executions at limit=100. M132 turns this route into
// the Live Wall's hot path, so the N+1 stops being survivable.
//
// New shape: a `page` CTE selects the fleet rows once (keyset-ordered), then
// each child table is aggregated ONCE with `GROUP BY`, scoped to that page's
// fleet ids via `IN (SELECT id FROM page)`. Two hash aggregates + two index
// scans replace 2×N subquery executions, and the cost no longer grows with
// page size. `LEFT JOIN` + `COALESCE(...,0)` reproduces the correlated
// subselect's zero for a fleet with no events / no telemetry (a `LEFT JOIN`
// miss would otherwise read NULL). Numbers are identical to the old query —
// pinned by `list_aggregate_integration_test.zig`.
//
// `$1` workspace_id, `$2` limit for the first page; the "after cursor" variant
// adds `$2` created_at + `$3` id and shifts limit to `$4`.

// Leading blank line so the CTE and this projection join with a newline
// without an explicit "\n" literal at each concatenation site.
const PAGE_SELECT_COLS =
    \\
    \\  SELECT p.id::text, p.name, p.status, p.created_at, p.updated_at,
    \\         (p.config_json->'x-agentsfleet'->'triggers')::text,
    \\         COALESCE(ev.events_processed, 0)::bigint,
    \\         COALESCE(te.budget_used_nanos, 0)::bigint
    \\  FROM page p
    \\  LEFT JOIN (
    \\    SELECT fleet_id, COUNT(*) AS events_processed
    \\    FROM core.fleet_events
    \\    WHERE fleet_id IN (SELECT id FROM page)
    \\    GROUP BY fleet_id
    \\  ) ev ON ev.fleet_id = p.id
    \\  LEFT JOIN (
    \\    SELECT fleet_id, SUM(credit_deducted_nanos) AS budget_used_nanos
    \\    FROM core.fleet_execution_telemetry
    \\    WHERE fleet_id IN (SELECT id::text FROM page)
    \\    GROUP BY fleet_id
    \\  ) te ON te.fleet_id = p.id::text
    \\  ORDER BY p.created_at DESC, p.id DESC
;

/// First page — the keyset's opening window.
pub const SELECT_FLEET_PAGE_FIRST =
    \\WITH page AS (
    \\  SELECT id, name, status, created_at, updated_at, config_json
    \\  FROM core.fleets
    \\  WHERE workspace_id = $1::uuid
    \\  ORDER BY created_at DESC, id DESC
    \\  LIMIT $2
    \\)
++ PAGE_SELECT_COLS;

/// Subsequent pages — keyset cursor on `(created_at, id)`.
pub const SELECT_FLEET_PAGE_AFTER =
    \\WITH page AS (
    \\  SELECT id, name, status, created_at, updated_at, config_json
    \\  FROM core.fleets
    \\  WHERE workspace_id = $1::uuid
    \\    AND (created_at < $2 OR (created_at = $2 AND id::text < $3))
    \\  ORDER BY created_at DESC, id DESC
    \\  LIMIT $4
    \\)
++ "\n" ++ PAGE_SELECT_COLS;
