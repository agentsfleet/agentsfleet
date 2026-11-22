-- Reads and assignments share conversation, FIFO and live-owner eligibility.
-- A live assignment is a barrier; it must never be skipped in favour of later input.
WITH origin AS MATERIALIZED (
    SELECT reply_provider, reply_address FROM thread.turns
    WHERE id = $3 AND thread_id = $1 AND workspace_id = $2
), conversation AS (
    SELECT a.id, a.sequence, a.request_json::text AS request_json, a.producer, a.actor, a.input_run_id, a.input_fence
    FROM thread.turns a CROSS JOIN origin o
    WHERE a.thread_id = $1 AND a.workspace_id = $2
      AND o.reply_provider IS NOT NULL
      AND a.reply_provider = o.reply_provider AND a.reply_address = o.reply_address
      AND a.settled_at IS NULL AND ($13 OR a.interpreted_at IS NULL)
    UNION ALL
    SELECT a.id, a.sequence, a.request_json::text AS request_json, a.producer, a.actor, a.input_run_id, a.input_fence
    FROM thread.turns a CROSS JOIN origin o
    WHERE a.thread_id = $1 AND a.workspace_id = $2
      AND o.reply_provider IS NULL
      AND a.reply_provider IS NULL AND a.reply_address IS NULL
      AND a.settled_at IS NULL AND ($13 OR a.interpreted_at IS NULL)
), candidates AS MATERIALIZED (
    SELECT a.id, a.sequence, a.request_json, a.producer,
           (a.input_run_id IS NULL
            OR (a.input_run_id = $9 AND a.input_fence = $10)
            OR NOT EXISTS (
                SELECT 1 FROM thread.runs assigned_run
                JOIN thread.threads s ON s.fleet_id = assigned_run.fleet_id
                WHERE assigned_run.id = a.input_run_id
                  AND assigned_run.fencing_token = a.input_fence
                  AND assigned_run.status = $11 AND assigned_run.run_expires_at > clock_timestamp()
                  AND s.last_runner_id = assigned_run.runner_id AND s.fencing_token = assigned_run.fencing_token
            )) AS assignable
    FROM conversation a
    WHERE ((a.producer = $4 AND a.actor <> $5 AND a.actor LIKE $6)
        OR (a.producer = $14 AND connector.slack_person_allowed($2, a.actor, $15, $16, $17)))
      AND a.id <> $3 AND a.sequence > $7
    ORDER BY a.sequence LIMIT $8
), eligible AS MATERIALIZED (
    SELECT id, sequence, request_json, producer FROM candidates
    WHERE assignable AND NOT EXISTS (
        SELECT 1 FROM candidates barrier WHERE NOT barrier.assignable AND barrier.sequence <= candidates.sequence)
), assigned AS (
    UPDATE thread.turns a
    SET input_run_id = $9, input_fence = $10, updated_at = now()
    FROM eligible e
    WHERE a.id = e.id AND a.id = $12
      AND e.sequence = (SELECT min(sequence) FROM eligible)
    RETURNING e.id, e.sequence, e.request_json, e.producer
)
SELECT id, sequence, request_json, producer FROM eligible WHERE $12 IS NULL
UNION ALL
SELECT id, sequence, request_json, producer FROM assigned
ORDER BY sequence
