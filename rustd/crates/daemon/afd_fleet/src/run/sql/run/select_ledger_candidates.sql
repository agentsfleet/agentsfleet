WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
), eligible AS MATERIALIZED (
  SELECT z.id, s.last_runner_id, instant.read_at, z.head_revision_id, host.platform_owned AS host_platform,
         (host.durable_tasks_enabled AND COALESCE(host.capability_report @> jsonb_build_object($8::text, true), false)) AS task_capable
  FROM fleet.fleets z
  CROSS JOIN instant
  JOIN runner.runners host ON host.id = $2
  LEFT JOIN thread.threads s ON s.fleet_id = z.id
  LEFT JOIN fleet.revisions setup ON setup.id = z.head_revision_id AND setup.fleet_id = z.id
  WHERE z.status = $1 AND host.cordon_reason IS NULL
    -- A tenant's runner takes only its tenant's work; the claim enforces it.
    AND (host.platform_owned OR host.tenant_id = z.tenant_id)
    AND ((host.durable_tasks_enabled AND COALESCE(host.capability_report @> jsonb_build_object($8::text, true), false))
         OR setup.definition #> $11::text[] IS NULL OR setup.definition #> $11::text[] = 'null'::jsonb)
    -- Table-wide: an empty ledger answers before any fleet is read.
    AND EXISTS (SELECT 1 FROM thread.turns WHERE settled_at IS NULL)
    -- Per fleet: only fleets with unsettled work reach the joins below, so the
    -- scan grows with the backlog rather than with the active fleets.
    AND EXISTS (
      SELECT 1 FROM thread.turns u WHERE u.thread_id = z.id AND u.settled_at IS NULL)
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
SELECT z.id AS fleet_id, head.id AS admission_id, head.created_at AS event_created_at
FROM eligible z
JOIN LATERAL (
  SELECT a.id, a.parked_at, a.created_at, a.task_id
  FROM thread.turns a
  WHERE a.thread_id = z.id AND a.settled_at IS NULL
    AND ((a.task_id IS NULL AND a.task_generation IS NULL) OR (z.task_capable AND EXISTS (
      SELECT 1 FROM fleet.tasks t WHERE t.id = a.task_id AND t.generation = a.task_generation AND t.fleet_id = a.thread_id
        AND t.workspace_id = a.workspace_id AND t.status IN ($9, $10) AND t.terminal_at IS NULL
        AND t.expires_at > z.read_at AND t.used_runs < t.max_runs
        AND t.used_model_nanos < t.max_model_nanos - t.reserved_model_nanos
        AND t.used_running_nanos < t.max_running_nanos)))
  ORDER BY a.sequence LIMIT 1

) head ON (head.parked_at IS NULL OR NOT EXISTS (
  (SELECT 1 FROM fleet.approval_gates g
  WHERE g.fleet_id = z.id AND g.resolved_at IS NULL
    AND g.event_id IS NULL LIMIT 1)
  UNION ALL
  (SELECT 1 FROM fleet.approval_gates g
  WHERE g.fleet_id = z.id AND g.resolved_at IS NULL
    AND g.event_id = head.id AND g.expires_at > z.read_at LIMIT 1)))
  -- A tenant runner skips a fleet whose head runs on the platform model.
  AND (z.host_platform OR EXISTS (
    SELECT 1 FROM fleet.revisions r
    WHERE r.fleet_id = z.id
      AND r.id = COALESCE(
        (SELECT t.accepted_revision_id FROM fleet.tasks t WHERE t.id = head.task_id AND t.fleet_id = z.id),
        z.head_revision_id)
      AND r.runtime_model_ref IS NOT NULL AND r.runtime_model_ref <> $16::text))
WHERE ($13::timestamptz IS NULL OR (head.created_at, head.id)>($13, $14::uuid))
  AND ($15::boolean OR head.task_id IS NOT NULL OR NOT EXISTS (
    SELECT 1 FROM runner.turn_recovery recovery
    WHERE recovery.turn_id = head.id AND recovery.head_turn_checkpoint_id IS NOT NULL
  ))
-- Recovery pages keep the oldest eligible head per fleet and ignore locality.
ORDER BY CASE WHEN $12 THEN head.created_at END,
         CASE WHEN $12 THEN head.id END,
         CASE WHEN NOT $12 THEN z.id = ANY($6) END DESC,
         CASE WHEN NOT $12 THEN z.id = $7 END DESC NULLS LAST,
         CASE WHEN NOT $12 THEN z.last_runner_id = $2 END DESC NULLS LAST,
         CASE WHEN NOT $12 THEN random() END
LIMIT $3
