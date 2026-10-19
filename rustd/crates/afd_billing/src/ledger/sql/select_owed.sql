SELECT COALESCE(SUM(charged_nanos) FILTER (WHERE work_type = $1), 0)::bigint AS fleet_nanos,
       COALESCE(SUM(charged_nanos) FILTER (WHERE work_type = $2), 0)::bigint AS authoring_nanos,
       COALESCE(EXTRACT(EPOCH FROM now() - MIN(created_at)), 0)::bigint AS oldest_seconds,
       (SELECT COALESCE(SUM(GREATEST(h.ceiling_nanos - h.captured_nanos, 0)), 0)::bigint
          FROM billing.holds h WHERE h.released_at IS NULL) AS held_nanos
FROM billing.usage_lines
WHERE settled_at IS NULL
