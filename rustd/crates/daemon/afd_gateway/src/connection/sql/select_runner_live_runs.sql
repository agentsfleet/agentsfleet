-- Reconnect/recheck only: no activity frame or ordinary scheduling pass runs this read.
-- Both branches use (runner_id, status, run_expires_at) index prefixes.
-- UNION ALL preserves the rows without a deduplication sort; slots uses its primary key.
-- A fleet's run carries its activity target (fleet, event); a draft's names its draft.
SELECT l.id AS run_id, l.run_expires_at AS expires_at,
       l.fleet_id, CASE WHEN l.fleet_id IS NOT NULL THEN l.event_id END AS event_id,
       l.created_at,
       CASE WHEN l.fleet_id IS NOT NULL THEN l.event_created_at END AS event_created_at,
       l.fencing_token, l.cancel_requested_at IS NOT NULL AS cancelled, l.draft_id
FROM thread.runs l
-- A slot's id is its owner's, fleet or draft.
JOIN runner.slots s ON s.id = COALESCE(l.fleet_id, l.draft_id)
WHERE l.runner_id = $1 AND l.status = $2
  AND l.run_expires_at > statement_timestamp()
  AND s.last_runner_id = l.runner_id AND s.fencing_token = l.fencing_token
  AND s.run_expires_at > statement_timestamp()
UNION ALL
-- Setup turns the authoring path still runs; this branch goes with setup.attempts.
SELECT a.run_id, LEAST(a.run_expires_at, a.expires_at),
       NULL, NULL, a.created_at, NULL, a.fencing_token, false, NULL
FROM setup.attempts a
WHERE a.runner_id = $1 AND a.status = $2
  AND a.run_expires_at > statement_timestamp() AND a.expires_at > statement_timestamp()
LIMIT $3
