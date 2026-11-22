SELECT t.id AS durable_task_id, t.status, t.generation, t.stop_generation,
       l.durable_task_generation, l.durable_task_stop_generation, t.accepted_revision_id,
       t.goal_digest, t.covered_input_sequence, t.input_sequence, t.effect_cursor, t.confirmed_effect_watermark,
       t.checkpoint_id, t.used_runs < t.max_runs AS can_resume,
       t.expires_at > clock_timestamp() AND t.used_model_nanos < t.max_model_nanos
         AND t.used_running_nanos < t.max_running_nanos AS within_limits,
       NOT EXISTS (SELECT 1 FROM fleet.durable_task_effects e
           WHERE e.durable_task_id = t.id AND e.status NOT IN ($3, $4)) AS effects_resolved
FROM thread.runs l JOIN fleet.durable_tasks t ON t.id = l.durable_task_id AND t.fleet_id = l.fleet_id AND t.workspace_id = l.workspace_id
WHERE l.id = $1 AND l.runner_id = $2
FOR UPDATE OF t
