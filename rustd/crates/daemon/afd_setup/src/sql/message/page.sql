SELECT m.id AS turn_id,
       m.sequence,
       m.save_requested,
       m.save_error_code,
       m.operation_id AS operation_id,
       m.status,
       m.request_json->>'message' AS user_text,
       m.answer_text,
       m.candidate_id AS candidate_id,
       m.failure_reason,
       (SELECT d.failure_cause FROM setup.attempts a
          JOIN setup.dispatches d ON d.attempt_id = a.id
         WHERE a.draft_id = m.thread_id AND a.message_sequence = m.sequence
           AND d.failure_cause IS NOT NULL
         ORDER BY d.sequence DESC LIMIT 1) AS failure_cause,
       m.created_at,
       m.settled_at AS finished_at,
       role.ordinal,
       -- A turn is answered by an attempt or by a draft run, never both, so at most one
       -- of `u` and `r` holds a row.
       COALESCE(u.reported_runtime_ms, r.reported_runtime_ms) AS reported_runtime_ms,
       COALESCE(u.input_tokens, r.input_tokens) AS input_tokens,
       COALESCE(u.output_tokens, r.output_tokens) AS output_tokens,
       COALESCE(u.provider_nanos, r.provider_nanos) AS provider_nanos,
       COALESCE(u.known, r.known) AS known
  FROM thread.turns m
 CROSS JOIN generate_series(0, 1) AS role(ordinal)
  LEFT JOIN LATERAL (
        SELECT a.reported_runtime_ms,
               (COALESCE(sum((d.usage->>'input_tokens')::bigint), 0) +
                COALESCE((SELECT sum((e.usage->>'input_tokens')::bigint) FROM setup.edit_decisions e
                           WHERE e.attempt_id = a.id), 0))::bigint AS input_tokens,
               (COALESCE(sum((d.usage->>'output_tokens')::bigint), 0) +
                COALESCE((SELECT sum((e.usage->>'output_tokens')::bigint) FROM setup.edit_decisions e
                           WHERE e.attempt_id = a.id), 0))::bigint AS output_tokens,
               (COALESCE(sum((d.usage->>'provider_cost_nanos')::bigint), 0) +
                COALESCE((SELECT sum(e.charged_nanos) FROM setup.edit_decisions e
                           WHERE e.attempt_id = a.id), 0))::bigint AS provider_nanos,
               COALESCE(bool_and(d.usage->>'knowledge' = $7), true)
                   AND count(d.id) FILTER (WHERE d.status IN ($8, $9)) = 0
                   AND NOT EXISTS (SELECT 1 FROM setup.edit_decisions e
                                    WHERE e.attempt_id = a.id AND e.accounted_at IS NULL) AS known
          FROM setup.attempts a
          LEFT JOIN setup.dispatches d ON d.attempt_id = a.id
         WHERE a.draft_id = m.thread_id AND a.message_sequence = m.sequence
           AND EXISTS (SELECT 1 FROM billing.setup_report_receipts rr
                        WHERE rr.attempt_id = a.id)
         GROUP BY a.id, a.reported_runtime_ms
       ) u ON true
  LEFT JOIN LATERAL (
        -- A draft run's usage, once no run of this turn is live: the lines every run
        -- logged, a rerun's included, keyed by the run as their work. Input is every
        -- prompt token the provider read or cached; the provider cost is every line but
        -- the runtime slices, which the runtime figure prices. Known only when each run
        -- reported: a lapsed run's requests were never reported.
        SELECT COALESCE(sum(l.runtime_ms), 0)::bigint AS reported_runtime_ms,
               COALESCE(sum(l.input_token_count + l.cached_input_token_count
                            + l.cache_write_token_count + l.cache_write_5m_token_count
                            + l.cache_write_1h_token_count), 0)::bigint AS input_tokens,
               COALESCE(sum(l.output_token_count), 0)::bigint AS output_tokens,
               COALESCE(sum(l.charged_nanos) FILTER (WHERE l.kind <> $11), 0)::bigint
                   AS provider_nanos,
               bool_and(x.status IN ($12, $13)) AS known
          FROM thread.runs x
          LEFT JOIN billing.usage_lines l ON l.work_type = $10 AND l.work_id = x.id
         WHERE x.turn_id = m.id AND x.draft_id = m.thread_id
        HAVING count(x.id) > 0 AND NOT bool_or(x.status = $14)
       ) r ON true
 WHERE m.thread_id = $1
   AND (m.sequence, role.ordinal) > ($2::bigint, $3::integer)
   AND (role.ordinal = 0 OR m.status NOT IN ($4, $5) OR m.answer_text IS NOT NULL)
 ORDER BY m.sequence, role.ordinal
 LIMIT $6
