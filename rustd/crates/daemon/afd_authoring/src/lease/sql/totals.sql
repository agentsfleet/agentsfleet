WITH chat AS (
    SELECT count(*)::bigint AS calls,
           COALESCE(sum((usage->>'input_tokens')::bigint), 0) AS input,
           COALESCE(sum((usage->>'output_tokens')::bigint), 0) AS output,
           COALESCE(sum((usage->>'provider_cost_nanos')::bigint), 0) AS provider,
           COALESCE(bool_and(usage->>'knowledge' = $2), true)
             AND count(*) FILTER (WHERE status IN ($3, $4)) = 0 AS known,
           count(*) FILTER (WHERE status = $5)::bigint AS failed_calls
    FROM authoring.dispatches WHERE attempt_id = $1::uuid
), decisions AS (
    SELECT COALESCE(sum((usage->>'input_tokens')::bigint), 0) AS input,
           COALESCE(sum((usage->>'output_tokens')::bigint), 0) AS output,
           COALESCE(sum(charged_nanos), 0) AS provider,
           count(*) FILTER (WHERE accounted_at IS NULL) = 0 AS known
    FROM authoring.edit_decisions WHERE attempt_id = $1::uuid
)
SELECT chat.calls, (chat.input + decisions.input)::bigint AS input,
       (chat.output + decisions.output)::bigint AS output,
       (chat.provider + decisions.provider)::bigint AS provider,
       chat.known AS chat_known, chat.known AND decisions.known AS known, chat.failed_calls
FROM chat CROSS JOIN decisions
