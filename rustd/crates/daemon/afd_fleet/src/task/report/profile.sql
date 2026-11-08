SELECT m.id FROM runner.decision_calls c
JOIN runner.leases l ON l.id = c.original_lease_id AND l.runner_id = c.original_runner_id
JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id AND s.last_runner_id = l.runner_id
JOIN runner.runners r ON r.id = l.runner_id AND r.status = $4
JOIN fleet.fleets f ON f.id = c.fleet_id AND f.workspace_id = c.workspace_id AND f.status = $5
JOIN model.platform_models m ON m.id = c.platform_model_id AND m.decision_enabled
JOIN model.decision_revisions d ON d.id = c.decision_revision_id AND d.platform_model_id = m.id
WHERE c.id = $1 AND l.id = $2 AND l.runner_id = $3
  AND c.accounted_at IS NOT NULL AND c.issued_at IS NOT NULL AND l.decision_snapshot = c.profile_snapshot
  AND l.fencing_token = c.fencing_token AND s.fencing_token = c.fencing_token
FOR SHARE OF m
