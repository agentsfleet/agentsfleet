WITH allocated AS (
    UPDATE fleet.tasks SET wait_sequence = COALESCE(wait_sequence, 0) + 1, updated_at = now()
    WHERE id = $2 RETURNING id, fleet_id, workspace_id, generation, stop_generation, wait_sequence, expires_at
), instant AS MATERIALIZED (SELECT clock_timestamp() AS at)
INSERT INTO fleet.task_waits
    (id, task_id, fleet_id, workspace_id, owner_lease_id, task_generation, stop_generation, generation,
     installation_id, repository_id, pull_request_number, expected_head, checks, baseline_digest,
     due_at, expires_at, cooldown_expires_at, fallback_count, signal_sequence, observed_signal_sequence, created_at, updated_at)
SELECT $1, a.id, a.fleet_id, a.workspace_id, $3, a.generation, a.stop_generation, a.wait_sequence,
    $4, $5, $6, $7, $8, $9, instant.at, a.expires_at, instant.at, 0, 0, 0, now(), now()
FROM allocated a CROSS JOIN instant
