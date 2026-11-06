UPDATE fleet.tasks
SET reserved_model_nanos = reserved_model_nanos + $4,
    used_decision_calls = used_decision_calls + 1,
    resource_version = resource_version + 1, updated_at = clock_timestamp()
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid
