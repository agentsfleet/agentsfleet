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
       m.created_at,
       m.settled_at AS finished_at,
       role.ordinal,
       r.reported_runtime_ms,
       r.input_tokens,
       r.output_tokens,
       r.provider_nanos,
       r.known
  FROM thread.turns m
 CROSS JOIN generate_series(0, 1) AS role(ordinal)
  LEFT JOIN LATERAL (
        -- A draft run's usage, once no run of this turn is live: the lines every run
        -- logged, a rerun's included, keyed by the run as their work. Input is every
        -- prompt token the provider read or cached; the provider cost is every line but
        -- the runtime slices, which the runtime figure prices. Known only when each run
        -- reported: a lapsed run's requests were never reported.
        --
        -- A run is live while it is active and its lease has not run out, the test the
        -- assignment reassigns a lapsed run's turn by (`setup_run/sql/candidates.sql`).
        -- Renewal never extends a lease past the run's runtime cap, so a run past its
        -- deadline is past its lease too. An active run nobody renews is not live: its
        -- usage shows, unknown, rather than staying hidden until the sweep expires it.
        SELECT COALESCE(sum(l.runtime_ms), 0)::bigint AS reported_runtime_ms,
               COALESCE(sum(l.input_token_count + l.cached_input_token_count
                            + l.cache_write_token_count + l.cache_write_5m_token_count
                            + l.cache_write_1h_token_count), 0)::bigint AS input_tokens,
               COALESCE(sum(l.output_token_count), 0)::bigint AS output_tokens,
               COALESCE(sum(l.charged_nanos) FILTER (WHERE l.kind <> $8), 0)::bigint
                   AS provider_nanos,
               bool_and(x.status IN ($9, $10)) AS known
          FROM thread.runs x
          LEFT JOIN billing.usage_lines l ON l.work_type = $7 AND l.work_id = x.id
         WHERE x.turn_id = m.id AND x.draft_id = m.thread_id
        HAVING count(x.id) > 0
           AND NOT bool_or(x.status = $11 AND x.run_expires_at > statement_timestamp())
       ) r ON true
 WHERE m.thread_id = $1
   AND (m.sequence, role.ordinal) > ($2::bigint, $3::integer)
   AND (role.ordinal = 0 OR m.status NOT IN ($4, $5) OR m.answer_text IS NOT NULL)
 ORDER BY m.sequence, role.ordinal
 LIMIT $6
