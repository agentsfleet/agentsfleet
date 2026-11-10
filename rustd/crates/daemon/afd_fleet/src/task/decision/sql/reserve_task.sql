UPDATE fleet.tasks
SET reserved_model_nanos = reserved_model_nanos + $4,
    used_decision_calls = used_decision_calls + 1,
    resource_version = resource_version + 1, updated_at = clock_timestamp()
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
