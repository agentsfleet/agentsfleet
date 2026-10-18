-- One join per source: the head revision carries the purpose and the trigger
-- projection, the counter row the lifetime totals. Both are keyed lookups
-- (fleet.revisions primary key, fleet.activity_counters primary key), so a page
-- costs one probe of each per row rather than a subquery per column.
SELECT f.id::text AS id,
       f.name,
       f.status,
       f.created_at,
       f.updated_at,
       r.definition->>'description' AS description,
       (r.definition->'config'->'x-agentsfleet'->'triggers')::text AS triggers,
       COALESCE(c.event_count, 0) AS event_count,
       COALESCE(c.budget_used_nanos, 0) AS budget_used_nanos
FROM fleet.fleets f
LEFT JOIN fleet.revisions r ON r.id = f.head_revision_id
LEFT JOIN fleet.activity_counters c ON c.fleet_id = f.id
WHERE f.workspace_id = $1::uuid
  AND (f.created_at, f.id) < (COALESCE($3::timestamptz, 'infinity'::timestamptz), $4::uuid)
ORDER BY f.created_at DESC, f.id DESC
LIMIT $2
