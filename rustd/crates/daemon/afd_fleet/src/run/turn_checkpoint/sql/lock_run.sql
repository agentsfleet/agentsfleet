WITH RECURSIVE held AS MATERIALIZED (
    SELECT l.id, l.runner_id, l.fleet_id, l.workspace_id, l.turn_id,
           l.setup_revision_id, l.fencing_token AS fence,
           l.run_expires_at, l.model, l.provider,
           s.fencing_token AS thread_fence, s.run_expires_at AS thread_expires_at,
           f.status AS fleet_status
    FROM thread.runs l
    JOIN thread.threads s
      ON s.fleet_id = l.fleet_id AND s.last_runner_id = l.runner_id
    JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
      AND l.durable_task_id IS NULL AND l.durable_task_generation IS NULL
    FOR UPDATE OF l
),
-- An answer's run ($4 is the answer's producer) saves and restores the checkpoint of the turn
-- that first asked. A question an answer's run asks names that answer, so its own answer
-- resumes it: the walk follows each answer to the turn it resumed until one is no answer. A
-- continuation resumes a turn written before it, so the walk ends.
asked AS (
    SELECT h.turn_id AS id, 0 AS depth FROM held h
    UNION ALL
    SELECT a.resumes_turn_id, s.depth + 1
    FROM asked s
    JOIN held h ON true
    JOIN thread.turns a ON a.id = s.id AND a.thread_id = h.fleet_id AND a.producer = $4
    WHERE a.resumes_turn_id IS NOT NULL
)
-- `turn_id` is the asking turn's, and `run_turn_id` the run's own.
SELECT h.id, h.fleet_id, h.workspace_id,
       (SELECT s.id FROM asked s ORDER BY s.depth DESC LIMIT 1) AS turn_id,
       h.turn_id AS run_turn_id, h.setup_revision_id,
       h.fence, h.run_expires_at, h.thread_fence, h.thread_expires_at, h.fleet_status,
       h.model, h.provider
FROM held h
