WITH measured AS MATERIALIZED (
    SELECT l.id, l.task_id, l.fleet_id, l.workspace_id,
        GREATEST(l.task_runtime_accounted_at, LEAST(l.last_metered_at, l.lease_expires_at)) AS until,
        (EXTRACT(EPOCH FROM GREATEST(l.task_runtime_accounted_at, LEAST(l.last_metered_at, l.lease_expires_at))
            - l.task_runtime_accounted_at) * $3::bigint)::bigint AS running_nanos
    FROM runner.leases l WHERE l.id = $1 AND l.task_id IS NOT NULL AND l.task_started_at IS NOT NULL
), cursor AS (
    UPDATE runner.leases l SET task_runtime_accounted_at = m.until, updated_at = now()
    FROM measured m WHERE l.id = m.id
    RETURNING l.id
), totals AS MATERIALIZED (
    SELECT t.id, t.used_model_nanos + $2 AS model_nanos, t.used_running_nanos + m.running_nanos AS running_nanos,
        t.terminal_at IS NULL AND (t.used_model_nanos + $2 >= t.max_model_nanos - t.reserved_model_nanos
            OR t.used_running_nanos + m.running_nanos >= t.max_running_nanos OR t.expires_at <= clock_timestamp()) AS stop
    FROM measured m JOIN fleet.tasks t ON t.id = m.task_id AND t.fleet_id = m.fleet_id AND t.workspace_id = m.workspace_id
    WHERE EXISTS (SELECT 1 FROM cursor)
)
UPDATE fleet.tasks t SET used_model_nanos = totals.model_nanos, used_running_nanos = totals.running_nanos,
    status = CASE WHEN totals.stop THEN $4 ELSE t.status END,
    terminal_at = CASE WHEN totals.stop THEN clock_timestamp() ELSE t.terminal_at END,
    generation = t.generation + totals.stop::integer, stop_generation = t.stop_generation + totals.stop::integer,
    resource_version = t.resource_version + 1,
    checkpoint_cleanup_at = CASE WHEN totals.stop THEN clock_timestamp() ELSE t.checkpoint_cleanup_at END, updated_at = now()
FROM totals WHERE t.id = totals.id
RETURNING t.status = $5 AND NOT totals.stop AS allowed
