WITH inserted AS (
  INSERT INTO runner.runners
    (id, host_id, token_hash, sandbox_tier, status, labels, network_policy,
     registry_allowlist, worker_count, degraded_reason, last_seen_at, lease_version,
     created_at, updated_at)
  VALUES ($1::uuid, $2::text, $3::text, $4::text, $5::text, $6::jsonb, $7::text,
          $8::jsonb, $9::int, $10::text, $11::timestamptz, $12::bigint, now(), now())
  RETURNING id, created_at
)
INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
SELECT $13::uuid, id, $14::text,
       jsonb_build_object($15::text, $2::text, $16::text, $4::text),
       NULL, created_at
FROM inserted
