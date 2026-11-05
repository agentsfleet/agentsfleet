SELECT t.id::text, t.originating_task_item_id::text AS originating_item_id, t.target::text,
       t.accepted_revision_id::text, r.revision AS accepted_revision_number, t.title, t.objective,
       t.questions::text, t.goal_digest, t.status AS state, t.resource_version, t.generation,
       t.stop_generation, t.covered_input_sequence, t.input_sequence, t.checkpoint_id::text,
       c.generation AS checkpoint_generation, t.effect_cursor,
       t.confirmed_effect_watermark, t.max_model_nanos, t.used_model_nanos,
       t.reserved_model_nanos, t.max_running_nanos, t.used_running_nanos,
       t.max_leases, t.used_leases, t.max_publications, t.used_publications,
       t.max_decision_calls, t.used_decision_calls, t.max_fallback_reads,
       t.used_fallback_reads, t.expires_at,
       EXTRACT(EPOCH FROM (t.expires_at - t.created_at))::int4 AS wall_seconds
FROM fleet.tasks t
JOIN fleet.revisions r ON r.id = t.accepted_revision_id AND r.fleet_id = t.fleet_id
LEFT JOIN fleet.task_checkpoints c ON c.id = t.checkpoint_id AND c.task_id = t.id
WHERE t.id = $1::uuid AND t.fleet_id = $2::uuid AND t.workspace_id = $3::uuid
