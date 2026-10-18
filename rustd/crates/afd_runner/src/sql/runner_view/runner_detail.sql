SELECT r.id::text AS id, r.host_id, r.sandbox_tier, r.status, r.labels::text AS labels,
       r.last_seen_at, r.created_at, statement_timestamp() AS checked_at,
       COALESCE(s.active_count, 0) > 0 AS live_lease,
       r.network_policy, r.registry_allowlist::text AS registry_allowlist, r.worker_count,
       r.capability_report::text AS capability_report, r.degraded_reason,
       r.sandbox_config::text AS sandbox_config,
       COALESCE(s.active_count, 0) AS active_lease_count,
       COALESCE(s.active_fleets, 0) AS active_fleet_count,
       COALESCE(c.acquired_count, 0) AS leases_acquired,
       COALESCE(c.succeeded_count, 0) AS leases_succeeded,
       COALESCE(c.failed_count, 0) AS leases_failed,
       COALESCE(c.expired_count, 0) AS leases_expired,
       r.selftest_requested_at, r.selftest_completed_at,
       r.selftest_checks::text AS selftest_checks,
       r.selftest_sandbox_tier, r.selftest_network_policy
FROM runner.runners r
LEFT JOIN (
  SELECT runner_id, COUNT(*)::bigint AS active_count,
         COUNT(DISTINCT fleet_id)::bigint AS active_fleets
  FROM runner.leases
  WHERE runner_id = $1::uuid AND status = $2::text
    AND lease_expires_at > statement_timestamp()
  GROUP BY runner_id
) s ON s.runner_id = r.id
LEFT JOIN runner.lifetime_counters c ON c.runner_id = r.id
WHERE r.id = $1::uuid
