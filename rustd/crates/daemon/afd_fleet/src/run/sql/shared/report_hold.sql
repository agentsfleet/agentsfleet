-- Receipt reads never renew or recreate a hold after a later issue consumed it.
SELECT s.fencing_token, s.hold_expires_at
FROM thread.runs l JOIN runner.slots s
  ON s.id = COALESCE(l.fleet_id, l.draft_id) AND s.last_runner_id = l.runner_id
 AND s.fencing_token = l.fencing_token
WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
  AND s.hold_expires_at > clock_timestamp()
