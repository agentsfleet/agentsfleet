//! SQL statement text for the fleet operator-plane handlers (RULE SQLMOD —
//! query text lives here, grepable in one place; siblings with pre-existing
//! inline SQL keep their shape until extracted).

/// Page-stable total for the operator runner list.
pub const SELECT_RUNNER_COUNT =
    \\SELECT COUNT(*)::bigint FROM fleet.runners
;

/// One keyset page of the operator runner list, newest first over the
/// composite `(created_at, id)` key. The lease-liveness `EXISTS` is evaluated
/// per page row only — index lookups scoped to the at-most-`limit` rows, never
/// a whole-table lease scan. `$1` lease status, `$2` now_ms.
const RUNNER_KEYSET_COLS =
    \\SELECT r.id::text, r.host_id, r.sandbox_tier, r.admin_state, r.labels::text,
    \\       r.last_seen_at, r.created_at,
    \\       EXISTS (
    \\           SELECT 1 FROM fleet.runner_leases l
    \\           WHERE l.runner_id = r.id AND l.status = $1 AND l.lease_expires_at > $2
    \\       ) AS has_live_lease,
    \\       r.network_policy, r.registry_allowlist::text, r.worker_count,
    \\       r.capability_report::text, r.degraded, r.degraded_reason,
    \\       r.extra_binds::text
    \\FROM fleet.runners r
    \\
;

/// `$3` limit.
pub const SELECT_RUNNER_KEYSET_FIRST = RUNNER_KEYSET_COLS ++
    \\ORDER BY r.created_at DESC, r.id DESC
    \\LIMIT $3
;

/// `$3` boundary created_at, `$4` boundary runner id, `$5` limit.
pub const SELECT_RUNNER_KEYSET_AFTER = RUNNER_KEYSET_COLS ++
    \\WHERE (r.created_at, r.id) < ($3::bigint, $4::uuid)
    \\ORDER BY r.created_at DESC, r.id DESC
    \\LIMIT $5
;

/// The single-runner operator read: the runner row plus a live-work summary and
/// lifetime counters, all from durable state in one statement.
///
/// Lifetime tallies come from `fleet.runner_lifetime_counters`, maintained by
/// the lease write paths in the same statements that write each transition —
/// the read is a one-to-one join, constant in the runner's history (the shape
/// `core.fleet_activity_counters` established). Only the live-now summary
/// still looks at `fleet.runner_leases`, scoped to currently-active rows via
/// the `(runner_id, status)` index — a set bounded by the worker count, never
/// by history. The counters survive retention pruning: they count transitions,
/// not surviving rows.
///
/// `$1` runner id, `$2` active lease status, `$3` now_ms.
pub const SELECT_RUNNER_DETAIL =
    \\SELECT r.id::text, r.host_id, r.sandbox_tier, r.admin_state, r.labels::text,
    \\       r.last_seen_at, r.created_at,
    \\       COALESCE(s.active_count, 0), COALESCE(s.active_fleets, 0),
    \\       COALESCE(c.acquired, 0), COALESCE(c.succeeded, 0),
    \\       COALESCE(c.failed, 0), COALESCE(c.expired, 0),
    \\       r.network_policy, r.registry_allowlist::text, r.worker_count,
    \\       r.capability_report::text, r.degraded, r.degraded_reason,
    \\       r.extra_binds::text,
    \\       r.selftest_requested_at, r.selftest_completed_at, r.selftest_checks::text,
    \\       r.selftest_all_ok, r.selftest_sandbox_tier, r.selftest_network_policy
    \\FROM fleet.runners r
    \\LEFT JOIN (
    \\    SELECT l.runner_id,
    \\           COUNT(*)::bigint AS active_count,
    \\           COUNT(DISTINCT l.fleet_id)::bigint AS active_fleets
    \\    FROM fleet.runner_leases l
    \\    WHERE l.runner_id = $1::uuid AND l.status = $2 AND l.lease_expires_at > $3
    \\    GROUP BY l.runner_id
    \\) s ON s.runner_id = r.id
    \\LEFT JOIN fleet.runner_lifetime_counters c ON c.runner_id = r.id
    \\WHERE r.id = $1::uuid
;

/// Existence probe plus the page-stable lease total in one round trip: no row
/// means the runner id does not resolve (404), a row carries the count every
/// page of the lease list reports as `total`. The NULL-guarded `$2` scopes the
/// total to one workspace when the list is filtered, so the pager and the rows
/// always describe the same set. `$3` narrows it further to one fleet on the
/// same terms, matching an id or an exact (case-insensitive) name. The fleets
/// join is on the primary key, so it cannot multiply the count. Retention bounds
/// the per-runner row count the COUNT walks.
pub const SELECT_RUNNER_LEASE_TOTAL =
    \\SELECT (SELECT COUNT(*) FROM fleet.runner_leases l
    \\        LEFT JOIN core.fleets f ON f.id = l.fleet_id
    \\        WHERE l.runner_id = r.id
    \\          AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
    \\          AND ($3::text IS NULL OR l.fleet_id::text = $3::text OR lower(f.name) = lower($3::text)))::bigint
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
///
/// The NULL-guarded workspace and fleet parameters narrow the page when the
/// operator filters; unfiltered calls bind NULL and pay nothing. The fleet
/// parameter matches an id or an exact (case-insensitive) fleet name, so the
/// operator filters by the name the table already shows them.
/// `$1` runner id, `$2` workspace id or NULL, `$3` limit, `$4` fleet id/name or NULL.
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
    \\  AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
    \\  AND ($4::text IS NULL OR l.fleet_id::text = $4::text OR lower(f.name) = lower($4::text))
    \\ORDER BY l.created_at DESC, l.id DESC
    \\LIMIT $3
;

/// The cursored continuation of `SELECT_RUNNER_LEASE_PAGE_FIRST`. The caller
/// resolves `starting_after` (a lease id) to its `(created_at, id)` pair first;
/// the row-value comparison then seeks strictly past it in the same composite
/// order. `$1` runner id, `$2` workspace id or NULL, `$3` boundary created_at,
/// `$4` boundary lease id, `$5` limit.
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
    \\  AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
    \\  AND ($6::text IS NULL OR l.fleet_id::text = $6::text OR lower(f.name) = lower($6::text))
    \\  AND (l.created_at, l.id) < ($3::bigint, $4::uuid)
    \\ORDER BY l.created_at DESC, l.id DESC
    \\LIMIT $5
;

/// Resolve a `starting_after` lease id to the composite sort key the page seek
/// needs. Scoped to the runner so a lease id from another runner is refused
/// rather than silently seeking into a foreign history — and scoped to the
/// SAME workspace and fleet filters the page carries (RULE KYS: a keyset cursor
/// names a position in one ordered stream, and the filters are part of what
/// defines it).
/// Without `$3` a cursor taken from workspace A would resolve to A's timestamp
/// and then seek B's page past it, silently dropping every B row newer than
/// that instant. `$1` lease id, `$2` runner id, `$3` workspace id or NULL.
pub const SELECT_RUNNER_LEASE_CURSOR =
    \\SELECT l.created_at FROM fleet.runner_leases l
    \\LEFT JOIN core.fleets f ON f.id = l.fleet_id
    \\WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
    \\  AND ($3::uuid IS NULL OR l.workspace_id = $3::uuid)
    \\  AND ($4::text IS NULL OR l.fleet_id::text = $4::text OR lower(f.name) = lower($4::text))
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
    \\    SELECT id, admin_state
    \\    FROM fleet.runners
    \\    WHERE id = $1::uuid
    \\), deleted AS (
    \\    DELETE FROM fleet.runners r
    \\    USING current_row c
    \\    WHERE r.id = c.id AND c.admin_state = $2::text
    \\    RETURNING r.id::text
    \\)
    \\SELECT d.id, TRUE AS changed
    \\FROM deleted d
    \\UNION ALL
    \\SELECT c.id::text, FALSE AS changed
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

/// Re-assign a runner's policy, its reconciled verdict, and the audit event
/// atomically.
///
/// `FOR UPDATE` serialises concurrent operator PATCHes; the `IS DISTINCT FROM`
/// guard makes a same-values re-assignment write nothing at all — no row, no
/// event — so the PATCH is idempotent and the history holds real changes only.
///
/// The verdict (`$13`/`$14`, computed by the caller against the row's stored
/// capability report) rides the SAME `UPDATE` as the assignment: a tightened
/// policy can never land beside a stale healthy verdict, which the lease gate
/// would read as "issue work". One statement, so there is no window and no
/// best-effort second write to fail.
pub const PATCH_RUNNER_ASSIGNED_POLICY =
    \\WITH current_p AS (
    \\  SELECT id, sandbox_tier, network_policy, registry_allowlist, worker_count, extra_binds
    \\  FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
    \\), updated AS (
    \\  UPDATE fleet.runners r
    \\  SET sandbox_tier = $2::text, network_policy = $3::text,
    \\      registry_allowlist = $4::jsonb, worker_count = $5::int, updated_at = $6::bigint,
    \\      degraded = $13::bool, degraded_reason = $14::text, extra_binds = $15::jsonb
    \\  FROM current_p c
    \\  WHERE r.id = c.id
    \\    AND (c.sandbox_tier IS DISTINCT FROM $2::text
    \\      OR c.network_policy IS DISTINCT FROM $3::text
    \\      OR c.registry_allowlist IS DISTINCT FROM $4::jsonb
    \\      OR c.worker_count IS DISTINCT FROM $5::int
    \\      OR c.extra_binds IS DISTINCT FROM $15::jsonb)
    \\  RETURNING r.id::text
    \\), event AS (
    \\  INSERT INTO fleet.runner_events
    \\    (id, runner_id, event_type, metadata, dedup_key, created_at)
    \\  SELECT $7::uuid, id::uuid, $8::text,
    \\         jsonb_build_object($9::text, $2::text, $10::text, $3::text,
    \\                            $11::text, $4::jsonb, $12::text, $5::int),
    \\         NULL, $6::bigint
    \\  FROM updated
    \\  RETURNING id
    \\)
    \\SELECT id FROM updated
;

/// The stored capability report alone — the PATCH path re-reconciles the new
/// assignment against it so the verdict never lags the assignment it judges.
pub const SELECT_RUNNER_CAPABILITY =
    \\SELECT capability_report::text FROM fleet.runners WHERE id = $1::uuid
;

/// Transition a runner's admin state and record the transition atomically.
///
/// `FOR UPDATE` serialises concurrent operator PATCHes so the recorded
/// `from_admin_state` is the true previous value rather than a racing read.
/// The `c.from_admin_state <> $2` guard makes a no-op transition write nothing
/// at all — no row, and therefore no event — so the history holds real changes
/// only.
/// Record an operator's outstanding self-test ask.
///
/// `selftest_requested_at IS NOT NULL` IS the pending state — the daemon clears
/// it on the beat that reports the matching verdict (`UPDATE_RUNNER_SELFTEST`),
/// which makes the ask one-shot without a status vocabulary in the schema.
///
/// Re-asking simply moves the instant forward: one runner holds at most one
/// outstanding request, so an impatient operator cannot queue a backlog of
/// probes against a host that is slow to beat.
///
/// The `admin_state <> $3` guard is the race the handler's own revoked check
/// cannot close — a runner revoked between that read and this write would
/// otherwise take a request it will never answer. Null row means gone or
/// revoked; the caller re-reads to tell those apart.
pub const PATCH_RUNNER_SELFTEST_REQUEST =
    \\UPDATE fleet.runners
    \\SET selftest_requested_at = $2::bigint, updated_at = $2::bigint
    \\WHERE id = $1::uuid AND admin_state <> $3::text
    \\RETURNING id::text
;

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
    \\    (id, runner_id, event_type, metadata, dedup_key, created_at)
    \\  SELECT $6::uuid, id::uuid, $7::text,
    \\         jsonb_build_object($8::text, from_admin_state, $9::text, $2::text),
    \\         NULL, $3::bigint
    \\  FROM updated
    \\  RETURNING id
    \\)
    \\SELECT id FROM updated
;
