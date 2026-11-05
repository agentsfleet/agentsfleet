SELECT h.work_id::text AS attempt_id, a.outcome
  FROM billing.holds h
  LEFT JOIN authoring.attempts a ON a.id = h.work_id
 WHERE h.work_type = $1 AND h.released_at IS NULL
   AND (a.id IS NULL OR a.status = $2)
 ORDER BY h.updated_at
 LIMIT $3
