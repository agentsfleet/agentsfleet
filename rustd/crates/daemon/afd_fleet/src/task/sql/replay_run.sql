WITH held AS MATERIALIZED (
    SELECT l.id AS run_id, l.fleet_id, l.workspace_id, l.tenant_id,
           l.turn_id AS source_turn_id, l.task_id, l.task_generation, l.task_stop_generation,
           l.fencing_token AS fence, s.fencing_token AS live_fence,
           f.head_revision_id AS revision_id,
           f.status AS fleet_status, f.name AS fleet_name,
           l.decision_snapshot, COALESCE(r.capability_report @> jsonb_build_object($4::text, true), false) AS capable,
           l.run_expires_at, s.run_expires_at AS thread_expires_at
    FROM thread.runs l
    JOIN runner.runners r ON r.id = l.runner_id
    JOIN thread.threads s ON s.fleet_id = l.fleet_id
    JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
      AND s.last_runner_id = l.runner_id
    FOR SHARE OF l
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM held
)
SELECT held.run_id, held.fleet_id, held.workspace_id, held.tenant_id,
       held.source_turn_id,
       held.task_id, held.task_generation, held.task_stop_generation, held.fence, held.live_fence, held.revision_id,
       held.fleet_status, held.fleet_name,
       held.decision_snapshot, held.capable, instant.checked_at, held.run_expires_at, held.thread_expires_at
FROM held CROSS JOIN instant
WHERE held.run_expires_at > instant.checked_at AND held.thread_expires_at > instant.checked_at
  AND (held.task_id IS NULL OR fleet.task_person_allowed(held.task_id, $5, $6, $7, $8))
