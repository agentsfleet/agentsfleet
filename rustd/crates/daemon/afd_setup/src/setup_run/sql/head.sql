-- The draft's head message under its lock: the oldest still queued or running, so
-- unsettled. A running head is reassigned whether or not a run row still names it, since a
-- runner's deletion cascades its runs away (`candidates.sql`).
--
-- $1 draft · $2 the queued status · $3 the running status
SELECT m.id, m.sequence, m.status, m.tenant_model_id, m.request_json->>'message' AS user_text, m.save_requested,
       m.created_at
  FROM thread.turns m
 WHERE m.thread_id = $1 AND m.status IN ($2, $3) AND m.settled_at IS NULL
 ORDER BY m.sequence
 LIMIT 1
