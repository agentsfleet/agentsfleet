INSERT INTO fleet.events
  (fleet_id, workspace_id, event_id, actor, event_type, status, request_json,
   created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb,
        coalesce($8::timestamptz, now()), coalesce($8::timestamptz, now()))
RETURNING event_id
