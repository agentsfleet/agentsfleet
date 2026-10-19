SELECT count(*)::bigint AS calls,
       COALESCE(sum((usage->>'input_tokens')::bigint), 0)::bigint AS input,
       COALESCE(sum((usage->>'output_tokens')::bigint), 0)::bigint AS output,
       COALESCE(sum((usage->>'provider_cost_nanos')::bigint), 0)::bigint AS provider,
       COALESCE(bool_and(usage->>'knowledge' = $2), true)
           AND count(*) FILTER (WHERE status IN ($3, $4)) = 0 AS known,
       count(*) FILTER (WHERE status = $5)::bigint AS failed_calls
  FROM authoring.dispatches
 WHERE attempt_id = $1::uuid
