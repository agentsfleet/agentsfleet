SELECT (COALESCE(sum((usage->>'provider_cost_nanos')::bigint), 0) +
        COALESCE((SELECT sum(COALESCE(e.charged_nanos, e.reserved_nanos))
                    FROM setup.edit_decisions e WHERE e.attempt_id = $1), 0))::bigint
 FROM setup.dispatches WHERE attempt_id = $1 AND usage->>'knowledge' = $2
