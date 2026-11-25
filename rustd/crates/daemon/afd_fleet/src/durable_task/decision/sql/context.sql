-- The reservation's read of its run: `../../sql/run.sql` with no lock, its deadlines read
-- against the statement's start. The issue statement (`issue.sql`) locks the thread and the run and
-- re-checks what this read decided, so nothing here holds a row across a round trip.
--
-- $1 run · $2 runner · $3 active run status · $4 durable-task capability · $5 Slack mention
-- producer · $6 Slack provider · $7 Slack person actor pattern · $8 its separator
WITH held AS MATERIALIZED (
    SELECT l.id AS run_id, l.fleet_id, l.workspace_id, l.tenant_id,
           l.turn_id AS source_turn_id, l.durable_task_id, l.durable_task_generation, l.durable_task_stop_generation,
           l.fencing_token AS fence, s.fencing_token AS live_fence,
           f.status AS fleet_status, f.name AS fleet_name,
           l.decision_snapshot, COALESCE(r.capability_report @> jsonb_build_object($4::text, true), false) AS capable,
           l.run_expires_at, s.run_expires_at AS thread_expires_at
    FROM thread.runs l
    JOIN runner.runners r ON r.id = l.runner_id
    JOIN thread.threads s ON s.fleet_id = l.fleet_id
    JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
      AND s.last_runner_id = l.runner_id
), instant AS MATERIALIZED (
    SELECT statement_timestamp() AS checked_at FROM held
)
SELECT held.run_id, held.fleet_id, held.workspace_id, held.tenant_id,
       held.source_turn_id,
       held.durable_task_id, held.durable_task_generation, held.durable_task_stop_generation, held.fence, held.live_fence,
       held.fleet_status, held.fleet_name,
       held.decision_snapshot, held.capable, instant.checked_at, held.run_expires_at, held.thread_expires_at
FROM held CROSS JOIN instant
WHERE held.run_expires_at > instant.checked_at AND held.thread_expires_at > instant.checked_at
  AND (held.durable_task_id IS NULL OR fleet.durable_task_person_allowed(held.durable_task_id, $5, $6, $7, $8))
