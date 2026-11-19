WITH held AS MATERIALIZED (
    SELECT l.id, l.runner_id, l.fleet_id, l.workspace_id, l.event_id,
           l.setup_revision_id, l.fencing_token AS fence,
           l.run_expires_at, l.model, l.provider,
           s.fencing_token AS slot_fence, s.run_expires_at AS slot_expires_at,
           f.status AS fleet_status
    FROM runner.runs l
    JOIN runner.slots s
      ON s.fleet_id = l.fleet_id AND s.last_runner_id = l.runner_id
    JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
      AND l.task_id IS NULL AND l.task_generation IS NULL
    FOR UPDATE OF l
)
SELECT id, fleet_id, workspace_id, event_id, setup_revision_id,
       fence, run_expires_at, slot_fence, slot_expires_at, fleet_status, model, provider
FROM held
