-- One new task, `ready`, while the fleet holds fewer unfinished tasks than its cap: no row past
-- the cap, which the caller refuses.
--
-- $1 id · $2 fleet · $3 workspace · $4 tenant · $5 originating item · $6 title · $7 objective ·
-- $8 questions · $9 goal digest · $10 target · $11 status · $12 model nanos · $13 running nanos ·
-- $14 runs · $15 publications · $16 decision calls · $17 reply provider · $18 reply address ·
-- $19 wall seconds · $20 the unfinished cap
WITH instant AS MATERIALIZED (
    SELECT date_trunc('milliseconds', clock_timestamp()) AS admitted_at
)
INSERT INTO fleet.durable_tasks
  (id, fleet_id, workspace_id, tenant_id, originating_durable_task_item_id,
   title, objective, questions, goal_digest, target, status, resource_version,
   generation, stop_generation, input_sequence, covered_input_sequence,
   effect_cursor, confirmed_effect_watermark, max_model_nanos, used_model_nanos,
   max_running_nanos, used_running_nanos, max_runs,
   used_runs, max_publications, used_publications, max_decision_calls,
   used_decision_calls, reply_provider, reply_address, expires_at, created_at, updated_at)
SELECT $1, $2, $3, $4, $5,
       $6, $7, $8::jsonb, $9, $10::jsonb, $11, 1, 1, 0, 0, 0, 0, 0,
       $12, 0, $13, 0, $14::bigint::integer, 0, $15::bigint::integer, 0, $16::bigint::integer, 0,
       $17, $18,
       instant.admitted_at + make_interval(secs => $19),
       instant.admitted_at, instant.admitted_at
FROM instant
WHERE (SELECT count(*) FROM fleet.durable_tasks
       WHERE fleet_id = $2 AND terminal_at IS NULL) < $20
RETURNING id
