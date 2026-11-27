-- The fleet's assignment. Its `won_thread` step is `../shared/assign_thread.sql`, fused here
-- with the expiry of the prior run and the fleet's turn, which the assignment stamps and opens
-- as the fleet's event in one update, so a chat's assignment stays one round trip: a change to
-- the thread's bookkeeping belongs in both.
WITH fleet AS MATERIALIZED (
  SELECT f.id, f.workspace_id, f.tenant_id, f.name, f.status, f.model_ref, f.http_egress,
         f.package_domains, f.memory_reads_workspace, f.memory_publishes_workspace
  FROM fleet.fleets f
  JOIN runner.runners placed ON placed.id = $2
  WHERE f.id = $1 AND f.status = $11
    -- A tenant's runner takes only its tenant's work. Without this row the thread
    -- below is never written, so a foreign assignment changes nothing.
    AND (placed.platform_owned OR placed.tenant_id = f.tenant_id)
), locked_thread AS MATERIALIZED (
  SELECT s.fleet_id FROM thread.threads s
  WHERE s.fleet_id = $1
  FOR NO KEY UPDATE OF s SKIP LOCKED
), instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM locked_thread) locked
), won_thread AS (
  INSERT INTO thread.threads AS s
    (id, fleet_id, last_runner_id, fencing_token, run_expires_at, park_expires_at,
     created_at, updated_at)
  SELECT $1, $1, $2, 1, instant.checked_at + $3::interval, NULL, now(), now()
  FROM instant
  WHERE EXISTS (SELECT 1 FROM fleet)
    AND EXISTS (SELECT 1 FROM runner.runners host WHERE host.id = $2 AND host.cordon_reason IS NULL)
    AND (EXISTS (SELECT 1 FROM locked_thread)
         OR NOT EXISTS (SELECT 1 FROM thread.threads WHERE fleet_id = $1))
  ON CONFLICT (id) DO UPDATE
    SET last_runner_id = EXCLUDED.last_runner_id,
        fencing_token = s.fencing_token + 1,
        run_expires_at = clock_timestamp() + $3::interval,
        park_expires_at = CASE WHEN s.last_runner_id = EXCLUDED.last_runner_id
                               THEN s.park_expires_at END,
        updated_at = now()
    WHERE s.run_expires_at < (SELECT checked_at FROM instant)
      AND (s.park_expires_at IS NULL
           OR s.park_expires_at <= (SELECT checked_at FROM instant)
           OR s.last_runner_id = $2
           OR NOT EXISTS (
             SELECT 1 FROM runner.runners r
             WHERE r.id = s.last_runner_id
               AND r.last_seen_at > (SELECT checked_at FROM instant) - $4::interval
               AND r.status = $5 AND r.degraded_reason IS NULL))
  RETURNING WITH (OLD AS prior) s.fencing_token, s.run_expires_at,
            prior.park_expires_at > (SELECT checked_at FROM instant) AS parked,
            prior.last_runner_id AS keeper
), first_head AS MATERIALIZED (
  SELECT a.id, a.durable_task_id, host.platform_owned AS host_platform FROM thread.turns a
  JOIN fleet f ON f.id = a.thread_id AND f.workspace_id = a.workspace_id
  JOIN runner.runners host ON host.id = $2
  WHERE a.thread_id = $1 AND a.settled_at IS NULL AND host.cordon_reason IS NULL
    AND ((a.durable_task_id IS NULL AND a.durable_task_generation IS NULL) OR EXISTS (
      SELECT 1 FROM fleet.durable_tasks t JOIN runner.runners r ON r.id = $2
      WHERE t.id = a.durable_task_id AND t.generation = a.durable_task_generation AND t.fleet_id = a.thread_id AND t.workspace_id = a.workspace_id
        AND r.durable_tasks_enabled AND r.capability_report @> jsonb_build_object($8::text, true)
        AND t.status IN ($9, $10) AND t.terminal_at IS NULL
        AND t.expires_at > (SELECT checked_at FROM instant) AND t.used_runs < t.max_runs
        AND t.used_model_nanos < t.max_model_nanos
        AND t.used_running_nanos < t.max_running_nanos))
  ORDER BY a.sequence
  LIMIT 1
), head AS MATERIALIZED (
  -- A tenant runner never receives the platform model key: the fleet must name the
  -- tenant's own model. Tested after the head is chosen, so a refused head blocks its
  -- fleet instead of being jumped.
  SELECT h.id FROM first_head h
  CROSS JOIN fleet f
  WHERE h.host_platform OR f.model_ref <> $14::text
), locked AS (
  SELECT a.id FROM thread.turns a
  JOIN head h ON h.id = a.id
  CROSS JOIN won_thread
  WHERE a.settled_at IS NULL
    AND ($13::boolean OR a.durable_task_id IS NOT NULL OR a.head_checkpoint_id IS NULL)
  FOR UPDATE OF a SKIP LOCKED
), stamped AS (
  -- Stamps the fence and, the first time a run takes the turn, opens its event: one
  -- update, since one statement may change a row once.
  UPDATE thread.turns a
  SET run_fence = won_thread.fencing_token,
      status = CASE WHEN a.opened_at IS NULL THEN $12 ELSE a.status END,
      opened_at = COALESCE(a.opened_at, now()),
      updated_at = now()
  FROM locked l, won_thread
  WHERE a.id = l.id AND a.settled_at IS NULL
  RETURNING WITH (OLD AS prior)
            a.id, a.workspace_id, a.actor, a.kind AS event_type, a.request_json::text AS request_json,
            a.created_at, a.durable_task_id, a.durable_task_generation,
            a.sequence, a.reply_provider, a.reply_address, a.status,
            prior.run_fence IS NOT NULL AS redelivered,
            prior.opened_at IS NULL AS inserted
), prior_run AS MATERIALIZED (
  SELECT l.id, l.runner_id, l.tenant_id, l.posture, l.model
  FROM thread.runs l JOIN stamped a ON a.id = l.turn_id
  WHERE l.fleet_id = $1
  ORDER BY l.fencing_token DESC LIMIT 1
  FOR NO KEY UPDATE OF l
), bumped AS (
  UPDATE thread.runs l SET status = $7, updated_at = now()
  FROM prior_run p WHERE l.id = p.id AND l.status = $6
  RETURNING l.runner_id
), counters AS (
  SELECT COALESCE(k.event_count, 0) + CASE WHEN a.inserted THEN 1 ELSE 0 END AS event_count,
         COALESCE(k.budget_used_nanos, 0) AS budget_used_nanos
  FROM stamped a
  LEFT JOIN fleet.activity_counters k ON k.fleet_id = $1
), installed AS (
  SELECT s.resume_context::text AS context_json
  FROM fleet f
  LEFT JOIN thread.threads s ON s.id = f.id
), catalogue AS MATERIALIZED (
  SELECT c.number FROM model.revision c WHERE c.id = 1
), platform AS MATERIALIZED (
  -- The active default's secret, chosen as the platform resolution chooses it,
  -- and the digest of what it holds: the same expression `platform.sql` computes
  -- over the ciphertext and the endpoint beside it, so a key read once is found
  -- again by the assignment's witnesses.
  SELECT s.id AS platform_secret_id,
         encode(sha256(convert_to(jsonb_build_array(
             s.meta_base_url, encode(sha256(s.secret_ciphertext), 'hex')
         )::text, 'UTF8')), 'hex') AS platform_secret_digest
  FROM model.platform_defaults p
  JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider
  WHERE p.active AND s.meta_has_key AND s.meta_provider = p.provider
  ORDER BY p.provider LIMIT 1
)
SELECT a.id AS turn_id, a.workspace_id, a.actor, a.event_type,
       a.request_json, a.created_at AS turn_created_at, a.redelivered,
       won_thread.fencing_token, won_thread.run_expires_at,
       COALESCE(won_thread.parked, false) AS parked, won_thread.keeper,
       b.id AS predecessor_run_id, b.id IS NOT NULL AS reassigned,
       -- The lapsed holder whose run this assignment expired, counted once it commits.
       (SELECT runner_id FROM bumped) AS expired_runner_id,
       -- The runs this turn was issued before this assignment. A report settles
       -- its turn, so each one is a run that lapsed without reporting.
       (SELECT count(*) FROM thread.runs x
         WHERE x.turn_id = a.id AND x.fleet_id = $1) AS earlier_runs,
       f.tenant_id, f.name, f.status, f.model_ref, f.http_egress, f.package_domains,
       f.memory_reads_workspace, f.memory_publishes_workspace, i.context_json,
       a.inserted, a.created_at AS opened_at, a.status AS event_status,
       k.event_count, k.budget_used_nanos,
       (SELECT number FROM catalogue) AS catalogue_version,
       pl.platform_secret_id, pl.platform_secret_digest,
       -- Whether the head belongs to a durable task: only such an issue reads
       -- its task source (`task/sql/issue_source.sql`).
       (a.durable_task_id IS NOT NULL OR a.durable_task_generation IS NOT NULL) AS tasked,
       -- Whether the host can judge a task: task-capable, with a decision profile selected.
       -- Only such a run is offered the task tools.
       (SELECT r.durable_tasks_enabled
               AND COALESCE(r.capability_report @> jsonb_build_object($8::text, true), false)
               AND r.decision_platform_model_id IS NOT NULL
          FROM runner.runners r WHERE r.id = $2) AS judged,
       -- Whether a later message waits in the head's conversation, foldable
       -- or not: the run's first fold ask finds nothing when this is false.
       -- assign_inputs.sql's two branches, through the same index. A durable
       -- task's inbox stays closed, so its run says false.
       a.durable_task_id IS NULL AND CASE
         WHEN a.reply_provider IS NOT NULL THEN EXISTS (
           SELECT 1 FROM thread.turns w
           WHERE w.thread_id = $1 AND w.reply_provider = a.reply_provider
             AND w.reply_address = a.reply_address
             AND w.settled_at IS NULL AND w.sequence > a.sequence)
         ELSE EXISTS (
           SELECT 1 FROM thread.turns w
           WHERE w.thread_id = $1 AND w.reply_provider IS NULL AND w.reply_address IS NULL
             AND w.settled_at IS NULL AND w.sequence > a.sequence)
       END AS inputs_waiting
FROM stamped a CROSS JOIN won_thread
LEFT JOIN prior_run b ON true
CROSS JOIN fleet f
CROSS JOIN counters k
CROSS JOIN installed i
LEFT JOIN platform pl ON true
