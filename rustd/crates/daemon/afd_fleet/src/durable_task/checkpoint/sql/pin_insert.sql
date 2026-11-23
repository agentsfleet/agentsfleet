WITH deadline AS MATERIALIZED (
    SELECT LEAST(t.expires_at, CASE WHEN $9 THEN c.expires_at
        ELSE clock_timestamp() + make_interval(secs => $10) END) AS at
    FROM fleet.durable_tasks t JOIN runner.checkpoints c ON c.durable_task_id = t.id
    WHERE t.id = $3 AND c.id = $2
)
INSERT INTO runner.checkpoint_pins (id, checkpoint_id, kind,
    owner_run_id, owner_fence, durable_task_generation, stop_generation, resource_version,
    expires_at, operation_expires_at, created_at, updated_at)
SELECT $1, $2, $4, $5, $6, $7, $8, 1,
    LEAST(deadline.at, clock_timestamp() + make_interval(secs => $11)), deadline.at, now(), now()
FROM deadline
