-- A seeded event has no admission behind it, so it is opened and settled at birth, as a
-- history-only receipt is: no claim takes it.
INSERT INTO thread.turns
  (thread_id, workspace_id, id, actor, kind, status, request_json,
   opened_at, settled_at, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb,
        coalesce($8::timestamptz, now()), coalesce($8::timestamptz, now()),
        coalesce($8::timestamptz, now()), coalesce($8::timestamptz, now()))
RETURNING id
