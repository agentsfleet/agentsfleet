SELECT id::text, accepted_revision_id::text, title, objective,
       questions::text, goal_digest, status AS state, resource_version, generation,
       stop_generation, covered_input_sequence, effect_cursor,
       confirmed_effect_watermark, max_model_nanos, used_model_nanos,
       reserved_model_nanos, max_running_nanos, used_running_nanos,
       max_leases, used_leases, max_publications, used_publications,
       max_decision_calls, used_decision_calls, max_fallback_reads,
       used_fallback_reads, expires_at,
       EXTRACT(EPOCH FROM (expires_at - created_at))::int4 AS wall_seconds
FROM fleet.tasks
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid
