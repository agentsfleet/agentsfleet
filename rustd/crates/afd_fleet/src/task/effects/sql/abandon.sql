UPDATE fleet.task_effect_states s SET status = $2, updated_at = now()
FROM fleet.task_effects e
WHERE s.effect_id = e.id AND e.id = $1::uuid AND s.status = $3
  AND (e.issuance_deadline <= clock_timestamp() OR NOT EXISTS (
      SELECT 1 FROM runner.leases l
      JOIN runner.fleet_slots slot ON slot.fleet_id = l.fleet_id
      JOIN fleet.tasks t ON t.id = e.task_id AND t.fleet_id = e.fleet_id AND t.workspace_id = e.workspace_id
      WHERE l.id = e.original_lease_id AND l.runner_id = e.original_runner_id
        AND l.fencing_token = e.original_fence AND l.status = $4
        AND l.lease_expires_at > clock_timestamp() AND slot.lease_expires_at > clock_timestamp()
        AND slot.last_runner_id = l.runner_id AND slot.fencing_token = e.original_fence
        AND t.generation = e.task_generation AND t.stop_generation = e.stop_generation
        AND t.status = $5 AND t.expires_at > clock_timestamp()
  ))
