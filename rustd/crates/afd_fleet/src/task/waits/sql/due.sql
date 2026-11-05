SELECT w.id::text, w.task_id::text, w.fleet_id::text, w.workspace_id::text
FROM fleet.task_waits w JOIN fleet.tasks t ON t.id = w.task_id
WHERE w.consumed_at IS NULL AND w.cancelled_at IS NULL
  AND (w.claim_id IS NULL OR w.claim_expires_at <= statement_timestamp())
  AND w.due_at <= statement_timestamp() AND w.cooldown_until <= statement_timestamp()
  AND t.status = $1 AND t.generation = w.task_generation AND t.stop_generation = w.stop_generation
  AND (w.initial_read_at IS NULL OR w.signal_sequence > w.observed_signal_sequence
       OR t.used_fallback_reads < t.max_fallback_reads OR t.expires_at <= statement_timestamp())
ORDER BY w.due_at, w.id LIMIT $2
