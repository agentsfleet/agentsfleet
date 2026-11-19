-- The draft's head message under its lock: the oldest above the Stop watermark still queued
-- or running, with whether a draft run ran it before. A running head no draft run ran
-- belongs to the authoring attempt path.
--
-- $1 draft · $2 the Stop watermark · $3 the queued status · $4 the running status
SELECT m.id, m.sequence, m.status, m.tenant_model_id, m.user_text, m.save_requested,
       m.created_at,
       EXISTS (SELECT 1 FROM thread.runs l
                WHERE l.event_id = m.id AND l.draft_id = m.draft_id) AS held
  FROM setup.messages m
 WHERE m.draft_id = $1 AND m.sequence > $2 AND m.status IN ($3, $4)
 ORDER BY m.sequence
 LIMIT 1
