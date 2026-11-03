WITH held AS MATERIALIZED (
    SELECT l.fleet_id::text, l.workspace_id::text, l.tenant_id::text,
           e.event_id AS source_admission_id, l.task_id::text,
           l.fencing_token AS fence, s.fencing_token AS live_fence,
           f.head_revision_id::text AS revision_id,
           l.lease_expires_at, s.lease_expires_at AS slot_expires_at
    FROM runner.leases l
    JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id
    JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
    JOIN fleet.events e ON e.id = l.fleet_event_id AND e.fleet_id = l.fleet_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $3
      AND s.last_runner_id = l.runner_id
    FOR UPDATE OF l
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM held
)
SELECT held.fleet_id, held.workspace_id, held.tenant_id, held.source_admission_id,
       held.task_id, held.fence, held.live_fence, held.revision_id
FROM held CROSS JOIN instant
WHERE held.lease_expires_at > instant.checked_at AND held.slot_expires_at > instant.checked_at
