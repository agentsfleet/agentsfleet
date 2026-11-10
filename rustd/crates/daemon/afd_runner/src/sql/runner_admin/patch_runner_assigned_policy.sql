WITH updated AS (
  UPDATE runner.runners
  SET sandbox_tier = $2::text, network_policy = $3::text,
      registry_allowlist = $4::jsonb, worker_count = $5::int,
      degraded_reason = $12::text, sandbox_config = $13::jsonb,
      decision_platform_model_id = $14, durable_tasks_enabled = $16::boolean, updated_at = now()
  WHERE id = $1
    AND (sandbox_tier IS DISTINCT FROM $2::text
      OR network_policy IS DISTINCT FROM $3::text
      OR registry_allowlist IS DISTINCT FROM $4::jsonb
      OR worker_count IS DISTINCT FROM $5::int
      OR sandbox_config IS DISTINCT FROM $13::jsonb
      OR decision_platform_model_id IS DISTINCT FROM $14
      OR durable_tasks_enabled IS DISTINCT FROM $16::boolean)
  RETURNING id
), event AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $6, id, $7::text,
         jsonb_build_object($8::text, $2::text, $9::text, $3::text,
                            $10::text, $4::jsonb, $11::text, $5::int,
                            $15::text, $14, $17::text, $16::boolean),
         NULL, now()
  FROM updated
)
SELECT id FROM updated
