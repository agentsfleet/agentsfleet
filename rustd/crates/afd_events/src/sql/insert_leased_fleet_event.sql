INSERT INTO fleet.events
  (fleet_id, event_id, workspace_id, actor, event_type,
   status, request_json, resumes_event_id, created_at, updated_at)
VALUES ($1::uuid, $2, $3::uuid, $4, $5, $8, $6::jsonb, $7, COALESCE((SELECT a.created_at FROM fleet.admissions a
            WHERE a.id = $9::uuid AND a.fleet_id = $1::uuid
              AND a.producer = $10 AND a.delivered_at IS NULL), now()), now())
ON CONFLICT (fleet_id, event_id) DO UPDATE SET
    resumes_event_id = COALESCE(fleet.events.resumes_event_id,
                                EXCLUDED.resumes_event_id)
RETURNING (xmax = 0) AS inserted, id::text AS id, created_at
