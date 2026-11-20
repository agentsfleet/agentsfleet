-- A fresh row is a history-only receipt: opened and settled at birth, so no claim takes it.
-- A conflict meets an admitted turn and opens its event, once.
INSERT INTO thread.turns AS t
  (id, thread_id, workspace_id, actor, kind, status, request_json, resumes_turn_id,
   opened_at, settled_at, created_at, updated_at)
VALUES (COALESCE($2, uuidv7()), $1, $3, $4, $5, $8, $6::jsonb, $7, now(), now(), now(), now())
ON CONFLICT (id) DO UPDATE SET
    status = CASE WHEN t.opened_at IS NULL THEN EXCLUDED.status ELSE t.status END,
    resumes_turn_id = COALESCE(t.resumes_turn_id, EXCLUDED.resumes_turn_id),
    updated_at = CASE WHEN t.opened_at IS NULL THEN now() ELSE t.updated_at END,
    opened_at = COALESCE(t.opened_at, EXCLUDED.opened_at)
RETURNING WITH (OLD AS prior) prior.opened_at IS NULL AS inserted
