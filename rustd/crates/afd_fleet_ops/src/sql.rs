//! SQL for read-only fleet operator projections.

pub(crate) const SELECT_RUNNER_LEASE_TOTAL: &str = "\
SELECT (SELECT COUNT(*) FROM fleet.runner_leases l
        LEFT JOIN core.fleets f ON f.id = l.fleet_id
        WHERE l.runner_id = r.id
          AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
          AND ($3::text IS NULL OR l.fleet_id::text = $3::text OR lower(f.name) = lower($3::text)))::bigint
FROM fleet.runners r
WHERE r.id = $1::uuid";

pub(crate) const SELECT_RUNNER_LEASE_CURSOR: &str = "\
SELECT l.created_at FROM fleet.runner_leases l
LEFT JOIN core.fleets f ON f.id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
  AND ($3::uuid IS NULL OR l.workspace_id = $3::uuid)
  AND ($4::text IS NULL OR l.fleet_id::text = $4::text OR lower(f.name) = lower($4::text))";

pub(crate) const SELECT_RUNNER_LEASE_PAGE_FIRST: &str = "\
SELECT l.id::text, l.fleet_id::text, f.name AS fleet_name, l.workspace_id::text,
       l.event_id, l.event_type, l.actor, l.status AS lease_status,
       l.lease_expires_at, l.created_at, l.fencing_token,
       l.provider, l.model, l.posture,
       l.metered_input_tokens, l.metered_cached_tokens, l.metered_output_tokens,
       e.status AS event_status, e.failure_label, e.failure_detail, e.wall_ms,
       EXISTS (
           SELECT 1 FROM fleet.runner_leases p
           WHERE p.fleet_id = l.fleet_id AND p.event_id = l.event_id
             AND p.fencing_token < l.fencing_token
       ) AS is_reclaim
FROM fleet.runner_leases l
LEFT JOIN core.fleets f ON f.id = l.fleet_id
LEFT JOIN core.fleet_events e ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
WHERE l.runner_id = $1::uuid
  AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
  AND ($4::text IS NULL OR l.fleet_id::text = $4::text OR lower(f.name) = lower($4::text))
ORDER BY l.created_at DESC, l.id DESC
LIMIT $3";

pub(crate) const SELECT_RUNNER_LEASE_PAGE_AFTER: &str = "\
SELECT l.id::text, l.fleet_id::text, f.name AS fleet_name, l.workspace_id::text,
       l.event_id, l.event_type, l.actor, l.status AS lease_status,
       l.lease_expires_at, l.created_at, l.fencing_token,
       l.provider, l.model, l.posture,
       l.metered_input_tokens, l.metered_cached_tokens, l.metered_output_tokens,
       e.status AS event_status, e.failure_label, e.failure_detail, e.wall_ms,
       EXISTS (
           SELECT 1 FROM fleet.runner_leases p
           WHERE p.fleet_id = l.fleet_id AND p.event_id = l.event_id
             AND p.fencing_token < l.fencing_token
       ) AS is_reclaim
FROM fleet.runner_leases l
LEFT JOIN core.fleets f ON f.id = l.fleet_id
LEFT JOIN core.fleet_events e ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
WHERE l.runner_id = $1::uuid
  AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
  AND ($6::text IS NULL OR l.fleet_id::text = $6::text OR lower(f.name) = lower($6::text))
  AND (l.created_at, l.id) < ($3::bigint, $4::uuid)
ORDER BY l.created_at DESC, l.id DESC
LIMIT $5";
