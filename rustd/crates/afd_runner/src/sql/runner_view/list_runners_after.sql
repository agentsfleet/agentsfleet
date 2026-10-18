SELECT r.id::text AS id, r.host_id, r.sandbox_tier, r.status, r.labels::text AS labels,
       r.last_seen_at, r.created_at, statement_timestamp() AS checked_at,
       EXISTS (
         SELECT 1 FROM runner.leases l
         WHERE l.runner_id = r.id AND l.status = $1::text
           AND l.lease_expires_at > statement_timestamp()
       ) AS live_lease,
       r.network_policy, r.registry_allowlist::text AS registry_allowlist, r.worker_count,
       r.capability_report::text AS capability_report, r.degraded_reason,
       r.sandbox_config::text AS sandbox_config
FROM runner.runners r
WHERE (r.created_at, r.id) < ($2::timestamptz, $3::uuid)
ORDER BY r.created_at DESC, r.id DESC
LIMIT $4::bigint
