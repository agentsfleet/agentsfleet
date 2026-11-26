WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
), eligible AS MATERIALIZED (
  -- A thread's last release, once an assignment has taken it; creation writes
  -- an expired deadline that releases nothing.
  SELECT z.id, s.last_runner_id,
         CASE WHEN s.fencing_token > 0 THEN s.run_expires_at END AS released_at,
         instant.read_at, z.model_ref,
         host.platform_owned AS host_platform,
         (host.durable_tasks_enabled AND COALESCE(host.capability_report @> jsonb_build_object($8::text, true), false)) AS durable_task_capable
  FROM fleet.fleets z
  CROSS JOIN instant
  JOIN runner.runners host ON host.id = $2
  LEFT JOIN thread.threads s ON s.fleet_id = z.id
  WHERE z.status = $1 AND host.cordon_reason IS NULL
    -- A tenant's runner takes only its tenant's work; the assignment enforces it.
    AND (host.platform_owned OR host.tenant_id = z.tenant_id)
    -- Table-wide: an empty ledger answers before any fleet is read.
    AND EXISTS (SELECT 1 FROM thread.turns WHERE settled_at IS NULL AND producer IS NOT NULL)
    -- Per fleet: only fleets with unsettled work reach the joins below, so the
    -- scan grows with the backlog rather than with the active fleets. A scalar
    -- subquery, not EXISTS: the planner never flattens it into a semi-join, so
    -- stale statistics cannot turn this probe into a walk of every unsettled turn.
    AND (SELECT true FROM thread.turns u WHERE u.thread_id = z.id AND u.settled_at IS NULL LIMIT 1)
    AND (s.run_expires_at IS NULL OR s.run_expires_at < instant.read_at)
    AND (s.hold_expires_at IS NULL
         OR s.hold_expires_at <= instant.read_at
         OR s.last_runner_id = $2
         OR NOT EXISTS (
           SELECT 1 FROM runner.runners r
           WHERE r.id = s.last_runner_id
             AND r.last_seen_at > instant.read_at - $4::interval
             AND r.status = $5 AND r.degraded_reason IS NULL))
    AND z.required_tags <@ (
      SELECT COALESCE(array_agg(e), '{}'::text[])
      FROM jsonb_array_elements_text(
        (SELECT CASE WHEN jsonb_typeof(labels) = 'array' THEN labels ELSE '[]'::jsonb END
         FROM runner.runners WHERE id = $2)) AS e)
)
SELECT z.id AS fleet_id, head.id AS turn_id, head.created_at AS turn_created_at
FROM eligible z
JOIN LATERAL (
  SELECT a.id, a.created_at, a.durable_task_id, a.head_checkpoint_id
  FROM thread.turns a
  WHERE a.thread_id = z.id AND a.settled_at IS NULL
    AND ((a.durable_task_id IS NULL AND a.durable_task_generation IS NULL) OR (z.durable_task_capable AND EXISTS (
      SELECT 1 FROM fleet.durable_tasks t WHERE t.id = a.durable_task_id AND t.generation = a.durable_task_generation AND t.fleet_id = a.thread_id
        AND t.workspace_id = a.workspace_id AND t.status IN ($9, $10) AND t.terminal_at IS NULL
        AND t.expires_at > z.read_at AND t.used_runs < t.max_runs
        AND t.used_model_nanos < t.max_model_nanos
        AND t.used_running_nanos < t.max_running_nanos)))
  ORDER BY a.sequence LIMIT 1

) head ON true
WHERE ($12::timestamptz IS NULL OR (head.created_at, head.id)>($12, $13::uuid))
  AND ($14::boolean OR head.durable_task_id IS NOT NULL OR head.head_checkpoint_id IS NULL)
  -- A tenant runner skips a fleet that runs on the platform model.
  AND (z.host_platform OR z.model_ref <> $15::text)
-- Recovery pages keep the oldest eligible head per fleet and ignore locality.
-- Ordinary passes take held fleets, the wake hint and this runner's last fleets
-- first, then the head that has waited longest while its thread was free: the
-- later of its queued time and the thread's last release. A head a pass freed
-- without a run so waits behind turns queued before that release. Runners
-- reading the same order skip each other at the assignment
-- (`assign_turn_head.sql`), which takes the thread and the head SKIP LOCKED.
ORDER BY CASE WHEN $11 THEN head.created_at END,
         CASE WHEN $11 THEN head.id END,
         CASE WHEN NOT $11 THEN z.id = ANY($6) END DESC,
         CASE WHEN NOT $11 THEN z.id = $7 END DESC NULLS LAST,
         CASE WHEN NOT $11 THEN z.last_runner_id = $2 END DESC NULLS LAST,
         CASE WHEN NOT $11 THEN GREATEST(head.created_at, z.released_at) END,
         CASE WHEN NOT $11 THEN head.id END
LIMIT $3
