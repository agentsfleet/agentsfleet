//! SQL statement text for the fleet operator-plane handlers (RULE SQLMOD —
//! query text lives here, grepable in one place; siblings with pre-existing
//! inline SQL keep their shape until extracted).

/// The operator runner list: one page, plus a total that survives an offset
/// past the end.
///
/// Pagination happens FIRST, in `page`; the lease-liveness `EXISTS` is
/// evaluated in `page_rows`, over the at-most-page_size rows that survive it.
/// The subquery used to sit in a CTE spanning the whole runner table, which
/// PostgreSQL answered by hashing the ENTIRE `runner_leases` table once per
/// request — 6 468 buffer hits against a 200 000-row lease table, against 75
/// for the page-scoped index lookups that replace it.
///
/// `total` keeps its meaning across the rewrite: `COUNT(*) OVER()` is a window
/// function, so it is computed over the full row set before `LIMIT` applies.
///
/// Two `{s}` slots, both the ORDER BY clause, and both fed from
/// `sortClauseFor`'s fixed allowlist — never from user input.
/// `$1` lease status, `$2` now_ms, `$3` limit, `$4` offset.
pub const SELECT_RUNNER_PAGE_FMT =
    \\WITH page AS (
    \\    SELECT r.id, r.host_id, r.sandbox_tier, r.admin_state, r.labels, r.last_seen_at, r.created_at,
    \\           COUNT(*) OVER()::bigint AS total,
    \\           ROW_NUMBER() OVER (ORDER BY {s})::bigint AS page_ord
    \\    FROM fleet.runners r
    \\    ORDER BY {s}
    \\    LIMIT $3 OFFSET $4
    \\),
    \\page_rows AS (
    \\    SELECT p.id::text, p.host_id, p.sandbox_tier, p.admin_state, p.labels::text, p.last_seen_at, p.created_at,
    \\           EXISTS (
    \\               SELECT 1
    \\               FROM fleet.runner_leases l
    \\               WHERE l.runner_id = p.id
    \\                 AND l.status = $1
    \\                 AND l.lease_expires_at > $2
    \\           ) AS has_live_lease,
    \\           p.total, false AS count_only, p.page_ord
    \\    FROM page p
    \\),
    \\total_row AS (
    \\    SELECT ''::text, ''::text, ''::text, 'active'::text, '[]'::text, 0::bigint, 0::bigint,
    \\           false, COUNT(*)::bigint, true, NULL::bigint
    \\    FROM fleet.runners
    \\    WHERE NOT EXISTS (SELECT 1 FROM page)
    \\)
    \\SELECT * FROM page_rows
    \\UNION ALL
    \\SELECT * FROM total_row
    \\ORDER BY count_only ASC, page_ord ASC NULLS LAST
;

// ── Operator-plane runner mutations ─────────────────────────────────────────

/// Delete a runner, reporting whether THIS call was the one that removed it.
///
/// The `admin_state = $2` guard on the DELETE arm means only a runner in the
/// expected state is removed, and the UNION returns the pre-existing row when
/// it was not — so a caller learns "already gone" or "wrong state" without a
/// separate read, and a live runner cannot be deleted out from under its leases.
pub const DELETE_RUNNER_IF_IN_STATE =
    \\WITH current_row AS (
    \\    SELECT uid, admin_state
    \\    FROM fleet.runners
    \\    WHERE id = $1::uuid
    \\), deleted AS (
    \\    DELETE FROM fleet.runners r
    \\    USING current_row c
    \\    WHERE r.uid = c.uid AND c.admin_state = $2::text
    \\    RETURNING r.uid::text
    \\)
    \\SELECT d.uid, TRUE AS changed
    \\FROM deleted d
    \\UNION ALL
    \\SELECT c.uid::text, FALSE AS changed
    \\FROM current_row c
    \\WHERE NOT EXISTS (SELECT 1 FROM deleted)
    \\LIMIT 1
;

pub const SELECT_RUNNER_EXISTS =
    \\SELECT 1 FROM fleet.runners WHERE id = $1::uuid
;

pub const SELECT_RUNNER_ADMIN_STATE =
    \\SELECT admin_state FROM fleet.runners WHERE id = $1::uuid
;

/// Transition a runner's admin state and record the transition atomically.
///
/// `FOR UPDATE` serialises concurrent operator PATCHes so the recorded
/// `from_admin_state` is the true previous value rather than a racing read.
/// The `c.from_admin_state <> $2` guard makes a no-op transition write nothing
/// at all — no row, and therefore no event — so the history holds real changes
/// only.
pub const PATCH_RUNNER_ADMIN_STATE =
    \\WITH current_state AS (
    \\  SELECT id, admin_state AS from_admin_state
    \\  FROM fleet.runners
    \\  WHERE id = $1::uuid
    \\  FOR UPDATE
    \\), updated AS (
    \\  UPDATE fleet.runners r
    \\  SET admin_state = $2::text, updated_at = $3::bigint
    \\  FROM current_state c
    \\  WHERE r.id = c.id
    \\    AND ($4::bool OR c.from_admin_state <> $5)
    \\    AND c.from_admin_state <> $2::text
    \\  RETURNING r.id::text, c.from_admin_state
    \\), event AS (
    \\  INSERT INTO fleet.runner_events
    \\    (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\  SELECT $6::uuid, id::uuid, $7::text, $3::bigint,
    \\         jsonb_build_object($8::text, from_admin_state, $9::text, $2::text),
    \\         NULL, $3::bigint
    \\  FROM updated
    \\  RETURNING id
    \\)
    \\SELECT id FROM updated
;
