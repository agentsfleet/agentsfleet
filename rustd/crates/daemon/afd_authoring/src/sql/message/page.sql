SELECT m.sequence,
       m.save_requested,
       m.save_error_code,
       m.operation_id AS operation_id,
       m.status,
       m.user_text,
       m.answer_text,
       m.candidate_id AS candidate_id,
       m.failure_reason,
       (SELECT d.failure_cause FROM authoring.attempts a
          JOIN authoring.dispatches d ON d.attempt_id = a.id
         WHERE a.draft_id = m.draft_id AND a.message_sequence = m.sequence
           AND d.failure_cause IS NOT NULL
         ORDER BY d.sequence DESC LIMIT 1) AS failure_cause,
       m.created_at,
       m.finished_at,
       role.ordinal,
       u.reported_runtime_ms,
       u.input_tokens,
       u.output_tokens,
       u.provider_nanos,
       u.known
  FROM authoring.messages m
 CROSS JOIN generate_series(0, 1) AS role(ordinal)
  LEFT JOIN LATERAL (
        SELECT a.reported_runtime_ms,
               (COALESCE(sum((d.usage->>'input_tokens')::bigint), 0) +
                COALESCE((SELECT sum((e.usage->>'input_tokens')::bigint) FROM authoring.edit_decisions e
                           WHERE e.attempt_id = a.id), 0))::bigint AS input_tokens,
               (COALESCE(sum((d.usage->>'output_tokens')::bigint), 0) +
                COALESCE((SELECT sum((e.usage->>'output_tokens')::bigint) FROM authoring.edit_decisions e
                           WHERE e.attempt_id = a.id), 0))::bigint AS output_tokens,
               (COALESCE(sum((d.usage->>'provider_cost_nanos')::bigint), 0) +
                COALESCE((SELECT sum(e.charged_nanos) FROM authoring.edit_decisions e
                           WHERE e.attempt_id = a.id), 0))::bigint AS provider_nanos,
               COALESCE(bool_and(d.usage->>'knowledge' = $7), true)
                   AND count(d.id) FILTER (WHERE d.status IN ($8, $9)) = 0
                   AND NOT EXISTS (SELECT 1 FROM authoring.edit_decisions e
                                    WHERE e.attempt_id = a.id AND e.accounted_at IS NULL) AS known
          FROM authoring.attempts a
          LEFT JOIN authoring.dispatches d ON d.attempt_id = a.id
         WHERE a.draft_id = m.draft_id AND a.message_sequence = m.sequence
           AND EXISTS (SELECT 1 FROM billing.authoring_report_receipts rr
                        WHERE rr.attempt_id = a.id)
         GROUP BY a.id, a.reported_runtime_ms
       ) u ON true
 WHERE m.draft_id = $1
   AND (m.sequence, role.ordinal) > ($2::bigint, $3::integer)
   AND (role.ordinal = 0 OR m.status NOT IN ($4, $5) OR m.answer_text IS NOT NULL)
 ORDER BY m.sequence, role.ordinal
 LIMIT $6
