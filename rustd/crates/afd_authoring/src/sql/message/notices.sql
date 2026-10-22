SELECT m.sequence,
       m.status,
       m.candidate_id::text AS candidate_id,
       m.failure_reason,
       (SELECT d.failure_cause FROM authoring.attempts a
          JOIN authoring.dispatches d ON d.attempt_id = a.id
         WHERE a.draft_id = m.draft_id AND a.message_sequence = m.sequence
           AND d.failure_cause IS NOT NULL
         ORDER BY d.sequence DESC LIMIT 1) AS failure_cause,
       u.reported_runtime_ms,
       u.input_tokens,
       u.output_tokens,
       u.provider_nanos,
       u.known
  FROM authoring.messages m
  LEFT JOIN LATERAL (
        SELECT a.reported_runtime_ms,
               COALESCE(sum((d.usage->>'input_tokens')::bigint), 0)::bigint AS input_tokens,
               COALESCE(sum((d.usage->>'output_tokens')::bigint), 0)::bigint AS output_tokens,
               COALESCE(sum((d.usage->>'provider_cost_nanos')::bigint), 0)::bigint AS provider_nanos,
               COALESCE(bool_and(d.usage->>'knowledge' = $3), true)
                   AND count(d.id) FILTER (WHERE d.status IN ($4, $5)) = 0 AS known
          FROM authoring.attempts a
          LEFT JOIN authoring.dispatches d ON d.attempt_id = a.id
         WHERE a.draft_id = m.draft_id AND a.message_sequence = m.sequence
           AND EXISTS (SELECT 1 FROM billing.authoring_report_receipts rr
                        WHERE rr.attempt_id = a.id)
         GROUP BY a.id, a.reported_runtime_ms
       ) u ON true
 WHERE m.draft_id = $1::uuid
 ORDER BY m.sequence DESC
 LIMIT $2
