//! Operator-plane runner and history reads.

/// Page-stable runner total.
pub const COUNT_RUNNERS: &str = "SELECT COUNT(*)::bigint FROM fleet.runners";

/// First runner page, newest first by the composite creation key.
pub const LIST_RUNNERS_FIRST: &str = "\
SELECT r.id::text, r.host_id, r.sandbox_tier, r.admin_state, r.labels::text,
       r.last_seen_at, r.created_at,
       EXISTS (
         SELECT 1 FROM fleet.runner_leases l
         WHERE l.runner_id = r.id AND l.status = $1::text
           AND l.lease_expires_at > $2::bigint
       ), r.network_policy, r.registry_allowlist::text, r.worker_count,
       r.capability_report::text, r.degraded, r.degraded_reason,
       r.extra_binds::text
FROM fleet.runners r
ORDER BY r.created_at DESC, r.id DESC
LIMIT $3::bigint";

/// Runner page strictly after a composite cursor.
pub const LIST_RUNNERS_AFTER: &str = "\
SELECT r.id::text, r.host_id, r.sandbox_tier, r.admin_state, r.labels::text,
       r.last_seen_at, r.created_at,
       EXISTS (
         SELECT 1 FROM fleet.runner_leases l
         WHERE l.runner_id = r.id AND l.status = $1::text
           AND l.lease_expires_at > $2::bigint
       ), r.network_policy, r.registry_allowlist::text, r.worker_count,
       r.capability_report::text, r.degraded, r.degraded_reason,
       r.extra_binds::text
FROM fleet.runners r
WHERE (r.created_at, r.id) < ($3::bigint, $4::uuid)
ORDER BY r.created_at DESC, r.id DESC
LIMIT $5::bigint";

/// One runner with current work counts and durable lifetime counters.
pub const RUNNER_DETAIL: &str = "\
SELECT r.id::text, r.host_id, r.sandbox_tier, r.admin_state, r.labels::text,
       r.last_seen_at, r.created_at,
       COALESCE(s.active_count, 0), COALESCE(s.active_fleets, 0),
       COALESCE(c.acquired, 0), COALESCE(c.succeeded, 0),
       COALESCE(c.failed, 0), COALESCE(c.expired, 0),
       r.network_policy, r.registry_allowlist::text, r.worker_count,
       r.capability_report::text, r.degraded, r.degraded_reason,
       r.extra_binds::text, r.selftest_requested_at,
       r.selftest_completed_at, r.selftest_checks::text,
       r.selftest_all_ok, r.selftest_sandbox_tier,
       r.selftest_network_policy
FROM fleet.runners r
LEFT JOIN (
  SELECT runner_id, COUNT(*)::bigint AS active_count,
         COUNT(DISTINCT fleet_id)::bigint AS active_fleets
  FROM fleet.runner_leases
  WHERE runner_id = $1::uuid AND status = $2::text
    AND lease_expires_at > $3::bigint
  GROUP BY runner_id
) s ON s.runner_id = r.id
LEFT JOIN fleet.runner_lifetime_counters c ON c.runner_id = r.id
WHERE r.id = $1::uuid";

/// Confirms the history owner exists.
pub const RUNNER_EXISTS: &str = "SELECT 1 FROM fleet.runners WHERE id = $1::uuid";

/// Page-stable history total.
pub const COUNT_EVENTS: &str = "\
SELECT COUNT(*)::bigint FROM fleet.runner_events
WHERE runner_id = $1::uuid
  AND ($2::text[] IS NULL OR event_type = ANY($2::text[]))
  AND ($3::bigint IS NULL OR created_at >= $3::bigint)
  AND ($4::bigint IS NULL OR created_at <= $4::bigint)";

/// First event page, newest first by the composite occurrence key.
pub const LIST_EVENTS_FIRST: &str = "\
SELECT id::text, runner_id::text, event_type, created_at, metadata::text
FROM fleet.runner_events
WHERE runner_id = $1::uuid
  AND ($2::text[] IS NULL OR event_type = ANY($2::text[]))
  AND ($3::bigint IS NULL OR created_at >= $3::bigint)
  AND ($4::bigint IS NULL OR created_at <= $4::bigint)
ORDER BY created_at DESC, id DESC
LIMIT $5::bigint";

/// Event page strictly after a composite cursor.
pub const LIST_EVENTS_AFTER: &str = "\
SELECT id::text, runner_id::text, event_type, created_at, metadata::text
FROM fleet.runner_events
WHERE runner_id = $1::uuid
  AND ($2::text[] IS NULL OR event_type = ANY($2::text[]))
  AND ($3::bigint IS NULL OR created_at >= $3::bigint)
  AND ($4::bigint IS NULL OR created_at <= $4::bigint)
  AND (created_at, id) < ($5::bigint, $6::uuid)
ORDER BY created_at DESC, id DESC
LIMIT $7::bigint";
