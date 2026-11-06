SELECT id::text AS id, status, host_id, sandbox_tier, last_seen_at, network_policy,
       registry_allowlist::text AS registry_allowlist, worker_count,
       capability_report::text AS capability_report, degraded_reason,
       sandbox_config::text AS sandbox_config,
       durable_tasks_enabled,
       decision_platform_model_id::text AS decision_model_id
FROM runner.runners
WHERE id = $1::uuid
