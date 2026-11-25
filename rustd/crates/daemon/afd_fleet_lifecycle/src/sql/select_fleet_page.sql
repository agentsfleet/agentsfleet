-- The counter row carries the lifetime totals, reached by its primary key
-- (fleet.activity_counters), so a page costs one probe per row rather than a
-- subquery per column.
SELECT f.id AS id,
       f.name,
       f.status,
       f.created_at,
       f.updated_at,
       COALESCE(c.event_count, 0) AS event_count,
       COALESCE(c.budget_used_nanos, 0) AS budget_used_nanos
FROM fleet.fleets f
LEFT JOIN fleet.activity_counters c ON c.fleet_id = f.id
WHERE f.workspace_id = $1
  AND (f.created_at, f.id) < (COALESCE($3::timestamptz, 'infinity'::timestamptz), $4)
ORDER BY f.created_at DESC, f.id DESC
LIMIT $2
