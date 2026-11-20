-- The draft's head message under its lock: the oldest still queued or running, so
-- unsettled, with whether a draft run ran it before. A running head no draft run ran is
-- not one a reassignment takes.
--
-- $1 draft · $2 the queued status · $3 the running status
SELECT m.id, m.sequence, m.status, m.tenant_model_id, m.request_json->>'message' AS user_text, m.save_requested,
       m.created_at,
       EXISTS (SELECT 1 FROM thread.runs l
                WHERE l.turn_id = m.id AND l.draft_id = m.thread_id) AS held
  FROM thread.turns m
 WHERE m.thread_id = $1 AND m.status IN ($2, $3) AND m.settled_at IS NULL
 ORDER BY m.sequence
 LIMIT 1
