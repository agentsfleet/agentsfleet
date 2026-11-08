SELECT f.head_revision_id AS revision_id, l.task_id, l.decision_snapshot
FROM runner.decision_calls c
JOIN runner.leases l ON l.id = c.original_lease_id AND l.runner_id = c.original_runner_id
JOIN runner.runners r ON r.id = l.runner_id AND r.status = $4
JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id AND s.last_runner_id = l.runner_id
JOIN fleet.fleets f ON f.id = c.fleet_id AND f.workspace_id = c.workspace_id
JOIN fleet.admissions a ON a.id = c.source_admission_id AND a.fleet_id = f.id
                       AND a.workspace_id = c.workspace_id AND a.settled_at IS NULL
JOIN model.platform_models m ON m.id = c.platform_model_id
                            AND m.decision_enabled
JOIN model.decision_revisions d ON d.id = c.decision_revision_id
                               AND d.platform_model_id = m.id
WHERE c.id = $1 AND l.fleet_id = f.id AND l.workspace_id = f.workspace_id
  AND l.status = $2 AND f.status = $3
  AND l.fencing_token = c.fencing_token AND s.fencing_token = c.fencing_token
  AND l.lease_expires_at > clock_timestamp() AND s.lease_expires_at > clock_timestamp()
  AND (l.event_id = c.source_admission_id
       OR (a.input_claim_lease_id = l.id AND a.input_claim_fence = c.fencing_token))
  AND (a.producer <> $5 OR connector.slack_person_allowed(a.workspace_id, a.actor, $6, $7, $8))
  AND (l.task_id IS NULL OR fleet.task_person_allowed(l.task_id, $5, $6, $7, $8))
