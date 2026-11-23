UPDATE fleet.durable_tasks
SET used_decision_calls = used_decision_calls + $6::boolean::integer,
    -- Exact charges remain in the financial header/ledger. Saturation prevents
    -- an over-limit late charge from losing accounting to an aggregate overflow.
    used_model_nanos = LEAST(used_model_nanos::numeric + $4, $5)::bigint,
    resource_version = resource_version + 1, updated_at = clock_timestamp()
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
RETURNING resource_version
