SELECT f.id::text AS id,
       f.name,
       f.status,
       f.created_at,
       f.updated_at,
       (SELECT (r.definition->'config'->'x-agentsfleet'->'triggers')::text
          FROM fleet.revisions r WHERE r.id = f.head_revision_id) AS triggers,
       COALESCE((SELECT c.event_count FROM fleet.activity_counters c
                  WHERE c.fleet_id = f.id), 0) AS event_count,
       COALESCE((SELECT c.budget_used_nanos FROM fleet.activity_counters c
                  WHERE c.fleet_id = f.id), 0) AS budget_used_nanos
FROM fleet.fleets f
WHERE f.workspace_id = $1::uuid
  AND (f.created_at, f.id) < (COALESCE($3::timestamptz, 'infinity'::timestamptz), $4::uuid)
ORDER BY f.created_at DESC, f.id DESC
LIMIT $2
