SELECT f.id::text AS id,
       f.workspace_id::text AS workspace_id,
       f.name,
       f.status,
       f.setup_stream_ready,
       f.initial_start::text AS initial_start,
       f.resource_version,
       f.created_at,
       f.updated_at,
       f.memory_reads_workspace,
       f.memory_publishes_workspace,
       r.revision AS head_revision,
       r.definition::text AS definition,
       r.review_context::text AS review_context,
       r.review_digest,
       r.source_candidate_id::text AS source_candidate_id,
       r.canonical_digest,
       r.policy_semantics,
       COALESCE(c.event_count, 0) AS events_processed,
       COALESCE(c.budget_used_nanos, 0) AS budget_used_nanos,
       (SELECT count(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = f.id AND g.resolved_at IS NULL) AS pending_approvals
  FROM fleet.fleets f
  JOIN fleet.revisions r ON r.id = f.head_revision_id
  LEFT JOIN fleet.activity_counters c ON c.fleet_id = f.id
 WHERE f.id = $1::uuid AND f.workspace_id = $2::uuid
