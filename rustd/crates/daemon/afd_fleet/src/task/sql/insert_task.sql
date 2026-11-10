WITH instant AS MATERIALIZED (
    SELECT date_trunc('milliseconds', clock_timestamp()) AS admitted_at
)
INSERT INTO fleet.tasks
  (id, fleet_id, workspace_id, tenant_id, originating_task_item_id, accepted_revision_id,
   title, objective, questions, goal_digest, target, status, resource_version,
   generation, stop_generation, input_sequence, covered_input_sequence,
   effect_cursor, confirmed_effect_watermark, max_model_nanos, used_model_nanos,
   reserved_model_nanos, max_running_nanos, used_running_nanos, max_leases,
   used_leases, max_publications, used_publications, max_decision_calls,
   used_decision_calls, max_fallback_reads, used_fallback_reads, reply_provider,
   reply_address, expires_at, created_at, updated_at)
SELECT $1, $2, $3, $4, $5, $6,
       $7, $8, $9::jsonb, $10, $11::jsonb, $12, 1, 1, 0, 0, 0, 0, 0,
       $13, 0, 0, $14, 0, $15::bigint::integer, $16, $17::bigint::integer, 0, $18::bigint::integer, 0, $19::bigint::integer, 0, $20, $21,
       instant.admitted_at + make_interval(secs => $22),
       instant.admitted_at, instant.admitted_at
FROM instant
WHERE (SELECT count(*) FROM fleet.tasks
       WHERE fleet_id = $2 AND terminal_at IS NULL) < $23
RETURNING id
