SELECT f.id::text AS id,
       COALESCE((SELECT c.event_count FROM fleet.activity_counters c
                  WHERE c.fleet_id = f.id), 0) AS event_count,
       COALESCE((SELECT c.budget_used_nanos FROM fleet.activity_counters c
                  WHERE c.fleet_id = f.id), 0) AS budget_used_nanos
FROM fleet.fleets f
WHERE f.workspace_id = $1::uuid AND f.id = ANY($2::uuid[])
