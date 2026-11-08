WITH deadline AS MATERIALIZED (
    SELECT LEAST(t.expires_at, CASE WHEN $11 THEN c.transfer_expires_at
        ELSE clock_timestamp() + make_interval(secs => $12) END) AS at
    FROM fleet.tasks t JOIN fleet.task_checkpoints c ON c.task_id = t.id
    WHERE t.id = $3::uuid AND c.id = $2::uuid
)
INSERT INTO fleet.task_object_pins (id, task_checkpoint_id, task_id, fleet_id, workspace_id, kind,
    owner_lease_id, owner_fence, task_generation, stop_generation, resource_version,
    expires_at, operation_expires_at, created_at, updated_at)
SELECT $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, $7::uuid, $8, $9, $10, 1,
    LEAST(deadline.at, clock_timestamp() + make_interval(secs => $13)), deadline.at, now(), now()
FROM deadline
