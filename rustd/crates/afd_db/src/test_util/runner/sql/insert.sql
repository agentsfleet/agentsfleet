INSERT INTO runner.runners
  (id, host_id, token_hash, sandbox_tier, status, labels, network_policy, registry_allowlist,
   worker_count, degraded_reason, last_seen_at, lease_version, created_at, updated_at)
VALUES ($1::uuid, $1::text, coalesce($12, $1::text), $2, $3, $4::jsonb, $5, $6::jsonb, $7, $8,
        coalesce($9::timestamptz, now()), $10, coalesce($11::timestamptz, now()),
        coalesce($11::timestamptz, now()))
