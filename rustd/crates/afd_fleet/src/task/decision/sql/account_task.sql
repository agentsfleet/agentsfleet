UPDATE fleet.tasks
SET reserved_model_nanos = reserved_model_nanos - $4,
    -- Exact charges remain in the financial header/ledger. Saturation prevents
    -- an over-limit late charge from losing accounting to an aggregate overflow.
    used_model_nanos = LEAST(used_model_nanos::numeric + $5, $6)::bigint,
    resource_version = resource_version + 1, updated_at = clock_timestamp()
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid
RETURNING resource_version
