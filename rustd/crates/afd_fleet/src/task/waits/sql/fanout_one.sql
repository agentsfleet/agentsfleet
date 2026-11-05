WITH held AS MATERIALIZED (
    SELECT completion_id, after_wait_id FROM fleet.task_completion_fanout
    WHERE completion_id = $1::uuid AND finished_at IS NULL AND next_attempt_at <= statement_timestamp()
    FOR UPDATE SKIP LOCKED
), source AS MATERIALIZED (
    SELECT c.workspace_id, c.scope_fleet_id, c.installation_id, c.repository_id,
        c.head_oid, c.workflow_id, c.run_id, c.run_number, c.run_attempt,
        c.received_at, h.after_wait_id
    FROM fleet.task_completions c JOIN held h ON h.completion_id = c.id
), target AS MATERIALIZED (
    SELECT w.id FROM fleet.task_waits w CROSS JOIN source c
    WHERE w.workspace_id = c.workspace_id
      AND w.installation_id = c.installation_id AND w.repository_id = c.repository_id AND w.expected_head = c.head_oid
      AND w.consumed_at IS NULL AND w.cancelled_at IS NULL AND w.id > COALESCE(c.after_wait_id, $2::uuid)
    ORDER BY w.id LIMIT 1 FOR UPDATE OF w
), signalled AS (
    UPDATE fleet.task_waits w SET signalled_at = GREATEST(w.signalled_at, c.received_at),
        signal_sequence = w.signal_sequence + 1,
        due_at = GREATEST(w.cooldown_until, LEAST(w.due_at, clock_timestamp())), updated_at = now()
    FROM source c, target t WHERE w.id = t.id
      AND (c.scope_fleet_id IS NULL OR w.fleet_id = c.scope_fleet_id)
      AND EXISTS (SELECT 1 FROM jsonb_to_recordset(w.checks)
          AS check_run(workflow_id bigint, run_id bigint, run_number bigint, run_attempt integer)
          WHERE check_run.workflow_id = c.workflow_id AND (check_run.run_id IS NULL
              OR check_run.run_id = c.run_id AND c.run_attempt >= check_run.run_attempt
              OR c.run_number > check_run.run_number))
    RETURNING w.id
)
UPDATE fleet.task_completion_fanout p SET after_wait_id = COALESCE((SELECT id FROM target), p.after_wait_id),
    finished_at = CASE WHEN NOT EXISTS (SELECT 1 FROM target) THEN clock_timestamp() ELSE p.finished_at END,
    next_attempt_at = clock_timestamp(), updated_at = now()
FROM held h WHERE p.completion_id = h.completion_id
RETURNING p.finished_at IS NOT NULL AS finished, (SELECT count(*)::bigint FROM signalled) AS signalled
