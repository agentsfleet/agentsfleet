SELECT sandbox_tier, network_policy, registry_allowlist,
       worker_count, degraded_reason, capability_report,
       selftest_requested_at IS NOT NULL AS selftest_requested,
       sandbox_config,
       durable_tasks_enabled,
       decision_platform_model_id AS decision_model_id,
       (token_hash = $2 OR $2 IS NULL) AS credential_matches,
       status = $4 AS active,
       (connection_id = $3 OR $3 IS NULL) AS owned
FROM runner.runners
WHERE id = $1
