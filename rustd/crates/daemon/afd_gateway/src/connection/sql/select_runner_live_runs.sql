-- Reconnect/recheck only: no activity frame or ordinary scheduling pass runs this read.
-- It uses the (runner_id, status, run_expires_at) index prefix; thread.threads uses its
-- primary key. A fleet's run carries its activity target (fleet, event); a draft's names
-- its draft.
SELECT l.id AS run_id, l.run_expires_at AS expires_at,
       l.fleet_id, CASE WHEN l.fleet_id IS NOT NULL THEN l.turn_id END AS turn_id,
       l.created_at,
       CASE WHEN l.fleet_id IS NOT NULL THEN l.turn_created_at END AS turn_created_at,
       l.fencing_token, l.cancel_requested_at IS NOT NULL AS cancelled, l.draft_id
FROM thread.runs l
-- A thread's id is its owner's, fleet or draft.
JOIN thread.threads s ON s.id = COALESCE(l.fleet_id, l.draft_id)
WHERE l.runner_id = $1 AND l.status = $2
  AND l.run_expires_at > statement_timestamp()
  AND s.last_runner_id = l.runner_id AND s.fencing_token = l.fencing_token
  AND s.run_expires_at > statement_timestamp()
LIMIT $3
