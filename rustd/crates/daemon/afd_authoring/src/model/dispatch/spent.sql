SELECT (COALESCE(sum((usage->>'provider_cost_nanos')::bigint), 0) +
        COALESCE((SELECT sum(COALESCE(e.charged_nanos, e.reserved_nanos))
                    FROM authoring.edit_decisions e WHERE e.attempt_id = $1::uuid), 0))::bigint
 FROM authoring.dispatches WHERE attempt_id = $1::uuid AND usage->>'knowledge' = $2
