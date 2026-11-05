WITH instant AS MATERIALIZED (
  SELECT statement_timestamp() AS read_at
), eligible AS MATERIALIZED (
  SELECT z.id, s.last_runner_id, instant.read_at,
         COALESCE(host.capability_report @> jsonb_build_object($8::text, true), false) AS task_capable
  FROM fleet.fleets z
  CROSS JOIN instant
  JOIN runner.runners host ON host.id = $2::uuid
  LEFT JOIN runner.fleet_slots s ON s.fleet_id = z.id
  LEFT JOIN fleet.revisions setup ON setup.id = z.head_revision_id AND setup.fleet_id = z.id
  WHERE z.status = $1
    AND (COALESCE(host.capability_report @> jsonb_build_object($8::text, true), false)
         OR setup.definition #> $11::text[] IS NULL OR setup.definition #> $11::text[] = 'null'::jsonb)
    AND EXISTS (SELECT 1 FROM fleet.admissions WHERE settled_at IS NULL)
    AND (s.lease_expires_at IS NULL OR s.lease_expires_at < instant.read_at)
    AND (s.hold_expires_at IS NULL
         OR s.hold_expires_at <= instant.read_at
         OR s.last_runner_id = $2::uuid
         OR NOT EXISTS (
           SELECT 1 FROM runner.runners r
           WHERE r.id = s.last_runner_id
             AND r.last_seen_at > instant.read_at - $4::interval
             AND r.status = $5 AND r.degraded_reason IS NULL))
    AND z.required_tags <@ (
      SELECT COALESCE(array_agg(e), '{}'::text[])
      FROM jsonb_array_elements_text(
        (SELECT CASE WHEN jsonb_typeof(labels) = 'array' THEN labels ELSE '[]'::jsonb END
         FROM runner.runners WHERE id = $2::uuid)) AS e)
)
SELECT z.id::text
FROM eligible z
JOIN LATERAL (
  SELECT a.id, a.parked_at
  FROM fleet.admissions a
  WHERE a.fleet_id = z.id AND a.settled_at IS NULL
    AND ((a.task_id IS NULL AND a.task_generation IS NULL) OR (z.task_capable AND EXISTS (
      SELECT 1 FROM fleet.tasks t WHERE t.id = a.task_id AND t.generation = a.task_generation AND t.fleet_id = a.fleet_id
        AND t.workspace_id = a.workspace_id AND t.status IN ($9, $10) AND t.terminal_at IS NULL
        AND t.expires_at > z.read_at AND t.used_leases < t.max_leases
        AND t.used_model_nanos < t.max_model_nanos - t.reserved_model_nanos
        AND t.used_running_nanos < t.max_running_nanos)))
  ORDER BY a.sequence LIMIT 1
) head ON head.parked_at IS NULL OR NOT EXISTS (
  (SELECT 1 FROM fleet.approval_gates g
  WHERE g.fleet_id = z.id AND g.resolved_at IS NULL
    AND g.event_id IS NULL LIMIT 1)
  UNION ALL
  (SELECT 1 FROM fleet.approval_gates g
  WHERE g.fleet_id = z.id AND g.resolved_at IS NULL
    AND g.event_id = head.id::text AND g.expires_at > z.read_at LIMIT 1))
ORDER BY z.id = ANY((SELECT ($6::text[])::uuid[])::uuid[]) DESC,
         (z.id = $7::uuid) DESC NULLS LAST,
         (z.last_runner_id = $2::uuid) DESC NULLS LAST,
         -- Only each fleet's head is ordered. Spread runners across fleets
         -- rather than making all of them contend for the same oldest fleet.
         random()
LIMIT $3
