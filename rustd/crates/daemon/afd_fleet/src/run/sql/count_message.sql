WITH run AS MATERIALIZED (
    SELECT l.id, l.fleet_id, l.posted_message_count, l.fencing_token, l.status,
           l.run_expires_at
    FROM thread.runs l
    WHERE l.id = $1
    FOR NO KEY UPDATE OF l
), live AS MATERIALIZED (
    SELECT run.id
    FROM run
    WHERE run.posted_message_count < $2
      AND run.fencing_token = $3 AND run.status = $4
      AND run.run_expires_at > clock_timestamp()
      AND run.fencing_token >= COALESCE(
            (SELECT a.fencing_token FROM runner.slots a WHERE a.fleet_id = run.fleet_id),
            run.fencing_token)
)
UPDATE thread.runs l
SET posted_message_count = l.posted_message_count + 1, updated_at = now()
FROM live
WHERE l.id = live.id
RETURNING l.posted_message_count
