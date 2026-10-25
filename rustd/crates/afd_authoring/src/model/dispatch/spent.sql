SELECT COALESCE(sum((usage->>'provider_cost_nanos')::bigint), 0)::bigint
 FROM authoring.dispatches WHERE attempt_id = $1::uuid AND usage->>'knowledge' = $2
