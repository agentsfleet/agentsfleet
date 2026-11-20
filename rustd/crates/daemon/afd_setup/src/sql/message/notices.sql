SELECT m.sequence,
       m.status,
       m.candidate_id AS candidate_id,
       m.failure_reason,
       r.reported_runtime_ms,
       r.input_tokens,
       r.output_tokens,
       r.provider_nanos,
       r.known
  FROM thread.turns m
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
               COALESCE(sum(l.charged_nanos) FILTER (WHERE l.kind <> $4), 0)::bigint
                   AS provider_nanos,
               bool_and(x.status IN ($5, $6)) AS known
          FROM thread.runs x
          LEFT JOIN billing.usage_lines l ON l.work_type = $3 AND l.work_id = x.id
         WHERE x.turn_id = m.id AND x.draft_id = m.thread_id
        HAVING count(x.id) > 0 AND NOT bool_or(x.status = $7)
       ) r ON true
 WHERE m.thread_id = $1
 ORDER BY m.sequence DESC
 LIMIT $2
