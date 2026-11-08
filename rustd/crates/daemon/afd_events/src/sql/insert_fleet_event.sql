INSERT INTO fleet.events
  (fleet_id, event_id, workspace_id, actor, event_type,
   status, request_json, resumes_event_id, created_at, updated_at)
VALUES ($1, COALESCE($2, uuidv7()), $3, $4, $5, $8, $6::jsonb, $7, now(), now())
ON CONFLICT (event_id) DO UPDATE SET
    resumes_event_id = COALESCE(fleet.events.resumes_event_id,
                                EXCLUDED.resumes_event_id)
RETURNING (xmax = 0) AS inserted, event_id
