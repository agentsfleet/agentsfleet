-- Receipt reads never renew or recreate a park after a later issue consumed it.
SELECT s.fencing_token, s.park_expires_at
FROM thread.runs l JOIN thread.threads s
  ON s.id = l.fleet_id AND s.last_runner_id = l.runner_id
 AND s.fencing_token = l.fencing_token
WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
  AND s.park_expires_at > clock_timestamp()
