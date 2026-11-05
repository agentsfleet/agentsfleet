SELECT t.id::text AS task_id, t.status, t.generation, t.stop_generation,
       l.task_generation, l.task_stop_generation, t.accepted_revision_id::text,
       t.goal_digest, t.covered_input_sequence, t.input_sequence, t.effect_cursor, t.confirmed_effect_watermark,
       t.checkpoint_id::text, t.used_leases < t.max_leases AS can_resume,
       t.expires_at > clock_timestamp() AND t.used_model_nanos < t.max_model_nanos - t.reserved_model_nanos
         AND t.used_running_nanos < t.max_running_nanos AS within_limits,
       NOT EXISTS (SELECT 1 FROM fleet.task_effects e JOIN fleet.task_effect_states s ON s.effect_id = e.id
           WHERE e.task_id = t.id AND s.status NOT IN ($3, $4)) AS effects_resolved
FROM runner.leases l JOIN fleet.tasks t ON t.id = l.task_id AND t.fleet_id = l.fleet_id AND t.workspace_id = l.workspace_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
FOR UPDATE OF t
