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

/// The single-runner operator read: the runner row plus a live-work summary and
/// lifetime counters, all from durable state in one statement.
///
/// The lease subquery scans the runner's whole lease history via the
/// `(runner_id, status)` index prefix — lifetime counting is the deliberate
/// trade (a windowed count would need an index that does not exist yet), and it
/// mirrors how `core.fleet_activity_counters` counts for Fleets. Succeeded and
/// failed split on the joined Fleet event's terminal status, and only for
/// leases the runner actually reported: an `expired` lease never inherits its
/// successor's outcome, and a stale `active` row past its deadline counts as
/// neither live nor expired until reclaim marks it.
///
/// `$1` runner id, `$2` active lease status, `$3` now_ms, `$4` reported lease
/// status, `$5` processed event status, `$6` fleet_error event status,
/// `$7` expired lease status.
pub const SELECT_RUNNER_DETAIL =
    \\SELECT r.id::text, r.host_id, r.sandbox_tier, r.admin_state, r.labels::text,
    \\       r.last_seen_at, r.created_at,
    \\       COALESCE(s.active_count, 0), COALESCE(s.active_fleets, 0),
    \\       COALESCE(s.acquired, 0), COALESCE(s.succeeded, 0),
    \\       COALESCE(s.failed, 0), COALESCE(s.expired, 0)
    \\FROM fleet.runners r
    \\LEFT JOIN (
    \\    SELECT l.runner_id,
    \\           COUNT(*) FILTER (WHERE l.status = $2 AND l.lease_expires_at > $3)::bigint AS active_count,
    \\           COUNT(DISTINCT l.fleet_id) FILTER (WHERE l.status = $2 AND l.lease_expires_at > $3)::bigint AS active_fleets,
    \\           COUNT(*)::bigint AS acquired,
    \\           COUNT(*) FILTER (WHERE l.status = $4 AND e.status = $5)::bigint AS succeeded,
    \\           COUNT(*) FILTER (WHERE l.status = $4 AND e.status = $6)::bigint AS failed,
    \\           COUNT(*) FILTER (WHERE l.status = $7)::bigint AS expired
    \\    FROM fleet.runner_leases l
    \\    LEFT JOIN core.fleet_events e ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
    \\    WHERE l.runner_id = $1::uuid
    \\    GROUP BY l.runner_id
    \\) s ON s.runner_id = r.id
    \\WHERE r.id = $1::uuid
;

/// Existence probe plus the page-stable lease total in one round trip: no row
/// means the runner id does not resolve (404), a row carries the count every
/// page of the lease list reports as `total`.
pub const SELECT_RUNNER_LEASE_TOTAL =
    \\SELECT (SELECT COUNT(*) FROM fleet.runner_leases l WHERE l.runner_id = r.id)::bigint
    \\FROM fleet.runners r
    \\WHERE r.id = $1::uuid
;

/// One lease page joined to its Fleet event (outcome + failure cause) and its
/// fleet (name for the link), newest first over the composite `(created_at, id)`
/// key so rows sharing a millisecond never skip across a page boundary.
///
/// `is_reclaim` derives from the fencing invariant rather than a stored column:
/// reclaim re-leases the SAME event under a strictly higher fencing token
/// (`fleet/reclaim.zig`), so a lower-fencing sibling lease for the same
/// `(fleet_id, event_id)` exists exactly when this lease is the reclaim.
///
/// Both fleet joins are LEFT so a decode never fabricates: a missing event row
/// reads as an unknown outcome, never a success.
/// `$1` runner id, `$2` limit.
pub const SELECT_RUNNER_LEASE_PAGE_FIRST =
    \\SELECT l.id::text, l.fleet_id::text, f.name, l.workspace_id::text,
    \\       l.event_id, l.event_type, l.actor, l.status,
    \\       l.lease_expires_at, l.created_at, l.fencing_token,
    \\       l.provider, l.model, l.posture,
    \\       l.metered_input_tokens, l.metered_cached_tokens, l.metered_output_tokens,
    \\       e.status, e.failure_label, e.failure_detail, e.wall_ms,
    \\       EXISTS (
    \\           SELECT 1 FROM fleet.runner_leases p
    \\           WHERE p.fleet_id = l.fleet_id AND p.event_id = l.event_id
    \\             AND p.fencing_token < l.fencing_token
    \\       ) AS is_reclaim
    \\FROM fleet.runner_leases l
    \\LEFT JOIN core.fleets f ON f.id = l.fleet_id
    \\LEFT JOIN core.fleet_events e ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
    \\WHERE l.runner_id = $1::uuid
    \\ORDER BY l.created_at DESC, l.id DESC
    \\LIMIT $2
;

/// The cursored continuation of `SELECT_RUNNER_LEASE_PAGE_FIRST`. The caller
/// resolves `starting_after` (a lease id) to its `(created_at, id)` pair first;
/// the row-value comparison then seeks strictly past it in the same composite
/// order. `$1` runner id, `$2` boundary created_at, `$3` boundary lease id,
/// `$4` limit.
pub const SELECT_RUNNER_LEASE_PAGE_AFTER =
    \\SELECT l.id::text, l.fleet_id::text, f.name, l.workspace_id::text,
    \\       l.event_id, l.event_type, l.actor, l.status,
    \\       l.lease_expires_at, l.created_at, l.fencing_token,
    \\       l.provider, l.model, l.posture,
    \\       l.metered_input_tokens, l.metered_cached_tokens, l.metered_output_tokens,
    \\       e.status, e.failure_label, e.failure_detail, e.wall_ms,
    \\       EXISTS (
    \\           SELECT 1 FROM fleet.runner_leases p
    \\           WHERE p.fleet_id = l.fleet_id AND p.event_id = l.event_id
    \\             AND p.fencing_token < l.fencing_token
    \\       ) AS is_reclaim
    \\FROM fleet.runner_leases l
    \\LEFT JOIN core.fleets f ON f.id = l.fleet_id
    \\LEFT JOIN core.fleet_events e ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
    \\WHERE l.runner_id = $1::uuid
    \\  AND (l.created_at, l.id) < ($2::bigint, $3::uuid)
    \\ORDER BY l.created_at DESC, l.id DESC
    \\LIMIT $4
;

/// Resolve a `starting_after` lease id to the composite sort key the page seek
/// needs. Scoped to the runner so a lease id from another runner is refused
/// rather than silently seeking into a foreign history.
pub const SELECT_RUNNER_LEASE_CURSOR =
    \\SELECT l.created_at FROM fleet.runner_leases l
    \\WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
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
