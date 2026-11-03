SELECT t.id::text, accepted_revision_id::text, r.revision AS accepted_revision_number, title, objective,
       questions::text, goal_digest, status AS state, resource_version, generation,
       stop_generation, covered_input_sequence, effect_cursor,
       confirmed_effect_watermark, max_model_nanos, used_model_nanos,
       reserved_model_nanos, max_running_nanos, used_running_nanos,
       max_leases, used_leases, max_publications, used_publications,
       max_decision_calls, used_decision_calls, max_fallback_reads,
       used_fallback_reads, expires_at,
       EXTRACT(EPOCH FROM (expires_at - t.created_at))::int4 AS wall_seconds
FROM fleet.tasks t
JOIN fleet.revisions r ON r.id = t.accepted_revision_id AND r.fleet_id = t.fleet_id
WHERE t.id = $1::uuid AND t.fleet_id = $2::uuid AND t.workspace_id = $3::uuid
