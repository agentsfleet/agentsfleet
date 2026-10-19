SELECT sandbox_tier, network_policy, registry_allowlist::text AS registry_allowlist,
       worker_count, degraded_reason, capability_report::text AS capability_report,
       selftest_requested_at IS NOT NULL AS selftest_requested,
       sandbox_config::text AS sandbox_config
FROM runner.runners
WHERE id = $1::uuid
