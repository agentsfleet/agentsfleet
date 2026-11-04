SELECT f.head_revision_id::text AS revision_id, l.task_id::text, l.decision_snapshot
FROM runner.decision_calls c
JOIN runner.leases l ON l.id = c.original_lease_id AND l.runner_id = c.original_runner_id
JOIN runner.runners r ON r.id = l.runner_id AND r.status = $4
JOIN runner.fleet_slots s ON s.fleet_id = l.fleet_id AND s.last_runner_id = l.runner_id
JOIN fleet.fleets f ON f.id = c.fleet_id AND f.workspace_id = c.workspace_id
JOIN fleet.events e ON e.id = l.fleet_event_id AND e.fleet_id = f.id
JOIN fleet.admissions a ON a.id = c.source_admission_id AND a.fleet_id = f.id
                       AND a.workspace_id = c.workspace_id AND a.settled_at IS NULL
JOIN model.platform_models m ON m.id = (c.profile_snapshot -> 'profile' ->> 'model_id')::uuid
                            AND m.decision_enabled
JOIN model.decision_revisions d ON d.id = (c.profile_snapshot ->> 'revision_id')::uuid
                               AND d.platform_model_id = m.id
WHERE c.id = $1::uuid AND l.fleet_id = f.id AND l.workspace_id = f.workspace_id
  AND l.status = $2 AND f.status = $3
  AND l.fencing_token = c.fencing_token AND s.fencing_token = c.fencing_token
  AND l.lease_expires_at > clock_timestamp() AND s.lease_expires_at > clock_timestamp()
  AND (e.event_id = c.source_admission_id::text
       OR (a.input_claim_lease_id = l.id AND a.input_claim_fence = c.fencing_token))
