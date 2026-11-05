WITH held AS MATERIALIZED (
    SELECT l.id::text AS lease_id, l.fleet_id::text, l.workspace_id::text, l.tenant_id::text,
           e.event_id AS source_admission_id, l.task_id::text, l.task_generation, l.task_stop_generation,
           l.fencing_token AS fence, s.fencing_token AS live_fence,
           f.head_revision_id::text AS revision_id,
           f.status AS fleet_status, f.name AS fleet_name,
           l.decision_snapshot, COALESCE(r.capability_report @> jsonb_build_object($4::text, true), false) AS capable,
           l.lease_expires_at, s.lease_expires_at AS slot_expires_at
    FROM runner.leases l
    JOIN runner.runners r ON r.id = l.runner_id
    JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id
    JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
    JOIN fleet.events e ON e.id = l.fleet_event_id AND e.fleet_id = l.fleet_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $3
      AND s.last_runner_id = l.runner_id
    FOR UPDATE OF l
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM held
)
SELECT held.lease_id, held.fleet_id, held.workspace_id, held.tenant_id, held.source_admission_id,
       held.task_id, held.task_generation, held.task_stop_generation, held.fence, held.live_fence, held.revision_id,
       held.fleet_status, held.fleet_name,
       held.decision_snapshot, held.capable, instant.checked_at, held.lease_expires_at, held.slot_expires_at
FROM held CROSS JOIN instant
WHERE held.lease_expires_at > instant.checked_at AND held.slot_expires_at > instant.checked_at
