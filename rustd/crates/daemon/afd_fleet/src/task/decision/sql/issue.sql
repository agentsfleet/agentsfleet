UPDATE runner.decision_calls c
SET status = $2, issued_at = clock_timestamp(), updated_at = clock_timestamp()
WHERE c.id = $1 AND c.status = $3 AND c.issued_at IS NULL AND c.accounted_at IS NULL
  AND c.logical_expires_at > clock_timestamp()
  AND EXISTS (
      SELECT 1 FROM runner.leases l
      JOIN runner.runners r ON r.id = l.runner_id AND r.status = $9
      JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id
      JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
      JOIN model.platform_models m ON m.id = $6 AND m.decision_enabled
      JOIN model.decision_revisions d ON d.id = $7 AND d.platform_model_id = m.id
      WHERE l.id = c.original_lease_id AND l.runner_id = c.original_runner_id
        AND l.status = $4 AND f.status = $5 AND f.id = c.fleet_id
        AND l.fencing_token = c.fencing_token AND s.fencing_token = c.fencing_token
        AND s.last_runner_id = l.runner_id
        AND l.lease_expires_at > clock_timestamp() AND s.lease_expires_at > clock_timestamp()
        AND (c.task_id IS NULL OR EXISTS (
            SELECT 1 FROM fleet.tasks t WHERE t.id = c.task_id
              AND t.fleet_id = c.fleet_id AND t.workspace_id = c.workspace_id
              AND t.generation = c.task_generation AND t.status = $8
              AND t.expires_at > clock_timestamp()
        ))
  )
RETURNING c.id
