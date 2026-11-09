SELECT sandbox_tier, network_policy, registry_allowlist,
       worker_count, degraded_reason, capability_report,
       selftest_requested_at IS NOT NULL AS selftest_requested,
       sandbox_config,
       durable_tasks_enabled,
       decision_platform_model_id AS decision_model_id,
       ($2 IS NULL OR token_hash = $2) AS credential_matches,
       status = $4 AS active,
       ($3 IS NULL OR connection_id = $3) AS owned
FROM runner.runners
WHERE id = $1
