SELECT l.id::text AS id,
       l.fleet_id::text AS fleet_id,
       f.name AS fleet_name,
       l.workspace_id::text AS workspace_id,
       e.event_id,
       e.event_type,
       e.actor,
       l.status AS lease_status,
       l.lease_expires_at,
       l.created_at,
       l.fencing_token,
       l.provider,
       l.model,
       l.posture,
       l.metered_input_token_count,
       l.metered_cached_token_count,
       l.metered_output_token_count,
       e.status AS event_status,
       e.failure_reason,
       e.failure_detail,
       e.wall_ms,
       EXISTS (
           SELECT 1 FROM runner.leases p
           WHERE p.fleet_event_id = l.fleet_event_id
             AND p.fencing_token < l.fencing_token
       ) AS is_reclaim
FROM runner.leases l
JOIN fleet.fleets f ON f.id = l.fleet_id
JOIN fleet.events e ON e.id = l.fleet_event_id
WHERE l.runner_id = $1::uuid
  AND ($2::uuid IS NULL OR l.workspace_id = $2::uuid)
  AND ($3::text IS NULL OR l.fleet_id::text = $3::text OR lower(f.name) = lower($3::text))
  AND (l.created_at, l.id) < (COALESCE($5::timestamptz, 'infinity'::timestamptz), $6::uuid)
ORDER BY l.created_at DESC, l.id DESC
LIMIT $4
