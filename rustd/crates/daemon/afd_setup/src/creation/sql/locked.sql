-- The fleet with its counters and open gates, as the caller's statement sees it: after
-- `lock.sql` for a write, and alone for a read that holds nothing.
--
-- $1 fleet · $2 workspace
SELECT f.id,
       f.name,
       f.status,
       f.model_ref,
       f.http_egress,
       f.package_domains,
       f.resource_version,
       f.created_at,
       f.updated_at,
       f.memory_reads_workspace,
       f.memory_publishes_workspace,
       COALESCE(c.event_count, 0) AS events_processed,
       COALESCE(c.budget_used_nanos, 0) AS budget_used_nanos,
       (SELECT count(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = f.id AND g.resolved_at IS NULL) AS pending_approvals
  FROM fleet.fleets f
  LEFT JOIN fleet.activity_counters c ON c.fleet_id = f.id
 WHERE f.id = $1 AND f.workspace_id = $2
