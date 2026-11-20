WITH registered AS (
  INSERT INTO runner.runners
    (id, host_id, token_hash, sandbox_tier, status, labels, network_policy,
     registry_allowlist, worker_count, degraded_reason, last_seen_at, run_version,
     decision_platform_model_id, sandbox_config, durable_tasks_enabled, platform_owned, tenant_id,
     created_at, updated_at)
  VALUES ($1, $2::text, $3::text, $4::text, $5::text, $6::jsonb, $7::text,
          $8::jsonb, $9::int, $10::text, $11::timestamptz, $12::bigint,
          $17, $19::jsonb, $20::boolean, $23::boolean, $22::uuid, now(), now())
  ON CONFLICT ON CONSTRAINT uq_runners_tenant_id_host_id
    DO UPDATE SET host_id = runner.runners.host_id
  RETURNING id, created_at, (xmax = 0) AS inserted
), enrolment AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $13, id, $14::text,
         jsonb_build_object($15::text, $2::text, $16::text, $4::text, $18::text, $17, $21::text, $20::boolean,
                           $24::text, $25::text),
         NULL, created_at
  FROM registered
  WHERE inserted
)
SELECT id, inserted FROM registered
