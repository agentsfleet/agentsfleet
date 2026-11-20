UPDATE fleet.task_effect_states s SET status = $2, updated_at = now()
FROM fleet.task_effects e
WHERE s.task_effect_id = e.id AND e.id = $1 AND s.status = $3
  AND (e.issuance_expires_at <= clock_timestamp() OR NOT EXISTS (
      SELECT 1 FROM thread.runs l
      JOIN thread.threads h ON h.fleet_id = l.fleet_id
      JOIN fleet.tasks t ON t.id = e.task_id AND t.fleet_id = e.fleet_id AND t.workspace_id = e.workspace_id
      WHERE l.id = e.original_run_id AND l.runner_id = e.original_runner_id
        AND l.fencing_token = e.original_fence AND l.status = $4
        AND l.run_expires_at > clock_timestamp() AND h.run_expires_at > clock_timestamp()
        AND h.last_runner_id = l.runner_id AND h.fencing_token = e.original_fence
        AND t.generation = e.task_generation AND t.stop_generation = e.stop_generation
        AND t.status = $5 AND t.expires_at > clock_timestamp()
  ))
