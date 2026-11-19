-- Kills the draft's live runs whose message a Stop just finished, so a runner stops
-- spending on a turn nobody reads. The mark is durable; the Cancel command is a hint.
--
-- $1 draft · $2 the Stop watermark · $3 active run status
UPDATE thread.runs l
   SET cancel_requested_at = COALESCE(l.cancel_requested_at, clock_timestamp()),
       updated_at = now()
  FROM setup.messages m
 WHERE l.draft_id = $1 AND l.status = $3
   AND m.id = l.event_id AND m.draft_id = l.draft_id AND m.sequence <= $2
RETURNING l.runner_id, l.id
