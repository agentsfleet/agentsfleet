WITH fleet AS MATERIALIZED (
  SELECT f.id, f.workspace_id, f.tenant_id, f.name, f.status, f.head_revision_id,
         f.memory_reads_workspace, f.memory_publishes_workspace
  FROM fleet.fleets f
  WHERE f.id = $1 AND f.status = $12
), locked_slot AS MATERIALIZED (
  SELECT s.fleet_id FROM runner.fleet_slots s
  WHERE s.fleet_id = $1
  FOR UPDATE OF s SKIP LOCKED
), instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM locked_slot) locked
), slot AS (
  INSERT INTO runner.fleet_slots AS s
    (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
     created_at, updated_at)
  SELECT $1, $2, 1, instant.checked_at + $3::interval, NULL, now(), now()
  FROM instant
  WHERE EXISTS (SELECT 1 FROM fleet)
    AND (EXISTS (SELECT 1 FROM locked_slot)
         OR NOT EXISTS (SELECT 1 FROM runner.fleet_slots WHERE fleet_id = $1))
  ON CONFLICT (fleet_id) DO UPDATE
    SET last_runner_id = EXCLUDED.last_runner_id,
        fencing_token = s.fencing_token + 1,
        lease_expires_at = clock_timestamp() + $3::interval,
        hold_expires_at = CASE WHEN s.last_runner_id = EXCLUDED.last_runner_id
                               THEN s.hold_expires_at END,
        updated_at = now()
    WHERE s.lease_expires_at < (SELECT checked_at FROM instant)
      AND (s.hold_expires_at IS NULL
           OR s.hold_expires_at <= (SELECT checked_at FROM instant)
           OR s.last_runner_id = $2
           OR NOT EXISTS (
             SELECT 1 FROM runner.runners r
             WHERE r.id = s.last_runner_id
               AND r.last_seen_at > (SELECT checked_at FROM instant) - $4::interval
               AND r.status = $5 AND r.degraded_reason IS NULL))
  RETURNING WITH (OLD AS prior) s.fencing_token, s.lease_expires_at,
            prior.hold_expires_at > (SELECT checked_at FROM instant) AS held,
            prior.last_runner_id AS holder
), head AS MATERIALIZED (
  SELECT a.id FROM fleet.admissions a
  JOIN fleet f ON f.id = a.fleet_id AND f.workspace_id = a.workspace_id
  LEFT JOIN fleet.revisions setup ON setup.id = f.head_revision_id AND setup.fleet_id = f.id
  JOIN runner.runners host ON host.id = $2
  WHERE a.fleet_id = $1 AND a.settled_at IS NULL
    AND ((host.durable_tasks_enabled AND COALESCE(host.capability_report @> jsonb_build_object($8::text, true), false))
         OR setup.definition #> $11::text[] IS NULL OR setup.definition #> $11::text[] = 'null'::jsonb)
    AND ((a.task_id IS NULL AND a.task_generation IS NULL) OR EXISTS (
      SELECT 1 FROM fleet.tasks t JOIN runner.runners r ON r.id = $2
      WHERE t.id = a.task_id AND t.generation = a.task_generation AND t.fleet_id = a.fleet_id AND t.workspace_id = a.workspace_id
        AND r.durable_tasks_enabled AND r.capability_report @> jsonb_build_object($8::text, true)
        AND t.status IN ($9, $10) AND t.terminal_at IS NULL
        AND t.expires_at > (SELECT checked_at FROM instant) AND t.used_leases < t.max_leases
        AND t.used_model_nanos < t.max_model_nanos - t.reserved_model_nanos
        AND t.used_running_nanos < t.max_running_nanos))
  ORDER BY a.sequence
  LIMIT 1
), locked AS (
  SELECT a.id FROM fleet.admissions a
  JOIN head h ON h.id = a.id
  CROSS JOIN slot
  WHERE a.settled_at IS NULL
  FOR UPDATE OF a SKIP LOCKED
), stamped AS (
  UPDATE fleet.admissions a
  SET lease_fence = slot.fencing_token,
      delivered_at = COALESCE(a.delivered_at, clock_timestamp()),
      updated_at = now()
  FROM locked l, slot
  WHERE a.id = l.id AND a.settled_at IS NULL
    AND (a.parked_at IS NULL OR NOT EXISTS (
      (SELECT 1 FROM fleet.approval_gates g
      WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL
        AND g.event_id IS NULL LIMIT 1)
      UNION ALL
      (SELECT 1 FROM fleet.approval_gates g
      WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL
        AND g.event_id = a.id
        AND g.expires_at > (SELECT checked_at FROM instant) LIMIT 1)))
  RETURNING WITH (OLD AS prior)
            a.id, a.workspace_id, a.actor, a.event_type, a.request_json,
            a.event_created_at, a.producer, a.created_at, a.task_id,
            a.sequence, a.reply_provider, a.reply_address,
            prior.delivered_at IS NOT NULL AS redelivered
), prior_lease AS MATERIALIZED (
  SELECT l.id, l.runner_id, l.tenant_id, l.posture, l.model
  FROM runner.leases l JOIN stamped a ON a.id = l.event_id
  WHERE l.fleet_id = $1
  ORDER BY l.fencing_token DESC LIMIT 1
  FOR UPDATE OF l
), bumped AS (
  UPDATE runner.leases l SET status = $7, updated_at = now()
  FROM prior_lease p WHERE l.id = p.id AND l.status = $6
  RETURNING l.runner_id
), opened AS (
  INSERT INTO fleet.events AS e
    (fleet_id, event_id, workspace_id, actor, event_type,
     status, request_json, resumes_event_id, created_at, updated_at)
  SELECT $1, a.id, a.workspace_id, a.actor, a.event_type, $13, a.request_json::jsonb, NULL,
         CASE WHEN a.producer = $14 AND NOT a.redelivered THEN a.created_at ELSE now() END,
         now()
  FROM stamped a
  ON CONFLICT (event_id) DO UPDATE SET
      resumes_event_id = COALESCE(e.resumes_event_id, EXCLUDED.resumes_event_id)
  RETURNING (e.xmax = 0) AS inserted, e.created_at, e.status
), counters AS (
  SELECT COALESCE(k.event_count, 0) + CASE WHEN o.inserted THEN 1 ELSE 0 END AS event_count,
         COALESCE(k.budget_used_nanos, 0) AS budget_used_nanos
  FROM opened o
  LEFT JOIN fleet.activity_counters k ON k.fleet_id = $1
), installed AS (
  SELECT r.definition::text AS definition, r.runtime_model_ref, s.context_json::text AS context_json,
         r.policy_semantics, r.id AS revision_id, t.generation AS task_generation,
         current.definition::text AS current_definition
  FROM stamped a
  CROSS JOIN fleet f
  LEFT JOIN fleet.tasks t ON t.id = a.task_id AND t.fleet_id = f.id AND t.workspace_id = f.workspace_id
  LEFT JOIN fleet.revisions r ON r.id = COALESCE(t.accepted_revision_id, f.head_revision_id)
   AND r.fleet_id = f.id
  LEFT JOIN fleet.revisions current ON current.id = f.head_revision_id AND current.fleet_id = f.id
  LEFT JOIN fleet.sessions s ON s.fleet_id = f.id
), catalogue AS MATERIALIZED (
  SELECT c.resource_version FROM model.catalogue_version c WHERE c.id = 1
), platform AS MATERIALIZED (
  -- The active default's secret, chosen as the platform resolution chooses it.
  SELECT s.id AS platform_secret_id, s.resource_version AS platform_secret_version
  FROM model.platform_defaults p
  JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
  WHERE p.active AND s.meta_has_key AND s.meta_provider = p.provider
  ORDER BY p.provider LIMIT 1
), tally AS (
  INSERT INTO runner.lifetime_counters AS c
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT runner_id, 0, 0, 0, 1, now(), now() FROM bumped
  ON CONFLICT (runner_id) DO UPDATE
    SET expired_count = c.expired_count + 1, updated_at = now()
)
SELECT a.id AS event_id, a.workspace_id, a.actor, a.event_type,
       a.request_json, a.event_created_at, a.redelivered,
       slot.fencing_token, slot.lease_expires_at,
       COALESCE(slot.held, false) AS held, slot.holder,
       b.tenant_id AS reused_tenant_id, b.posture AS reused_posture,
       b.model AS reused_model,
       f.tenant_id, f.name, f.status, f.head_revision_id,
       f.memory_reads_workspace, f.memory_publishes_workspace,
       i.definition, i.runtime_model_ref, i.context_json, i.policy_semantics,
       i.revision_id, i.task_generation, i.current_definition,
       o.inserted, o.created_at AS opened_at, o.status AS event_status,
       k.event_count, k.budget_used_nanos,
       (SELECT resource_version FROM catalogue) AS catalogue_version,
       pl.platform_secret_id, pl.platform_secret_version,
       -- Whether a later message waits in the head's conversation, foldable
       -- or not: the run's first fold ask finds nothing when this is false.
       -- claim_inputs.sql's two branches, through the same index. A durable
       -- task's inbox stays closed, so its lease says false.
       a.task_id IS NULL AND CASE
         WHEN a.reply_provider IS NOT NULL THEN EXISTS (
           SELECT 1 FROM fleet.admissions w
           WHERE w.fleet_id = $1 AND w.reply_provider = a.reply_provider
             AND w.reply_address = a.reply_address
             AND w.settled_at IS NULL AND w.sequence > a.sequence)
         ELSE EXISTS (
           SELECT 1 FROM fleet.admissions w
           WHERE w.fleet_id = $1 AND w.reply_provider IS NULL AND w.reply_address IS NULL
             AND w.settled_at IS NULL AND w.sequence > a.sequence)
       END AS inputs_waiting
FROM stamped a CROSS JOIN slot
LEFT JOIN prior_lease b ON true
CROSS JOIN fleet f
CROSS JOIN opened o
CROSS JOIN counters k
CROSS JOIN installed i
LEFT JOIN platform pl ON true
