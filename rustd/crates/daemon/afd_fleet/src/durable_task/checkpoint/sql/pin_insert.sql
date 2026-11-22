WITH deadline AS MATERIALIZED (
    SELECT LEAST(t.expires_at, CASE WHEN $11 THEN c.transfer_expires_at
        ELSE clock_timestamp() + make_interval(secs => $12) END) AS at
    FROM fleet.durable_tasks t JOIN fleet.durable_task_checkpoints c ON c.durable_task_id = t.id
    WHERE t.id = $3 AND c.id = $2
)
INSERT INTO fleet.durable_task_object_pins (id, durable_task_checkpoint_id, durable_task_id, fleet_id, workspace_id, kind,
    owner_run_id, owner_fence, durable_task_generation, stop_generation, resource_version,
    expires_at, operation_expires_at, created_at, updated_at)
SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1,
    LEAST(deadline.at, clock_timestamp() + make_interval(secs => $13)), deadline.at, now(), now()
FROM deadline
