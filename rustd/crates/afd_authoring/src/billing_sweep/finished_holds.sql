SELECT h.work_id::text AS attempt_id, a.outcome
  FROM billing.holds h
  LEFT JOIN authoring.attempts a ON a.id = h.work_id
 WHERE h.work_type = $1 AND h.released_at IS NULL
   AND (a.id IS NULL OR (a.status = $2 AND a.updated_at < now() - $3::interval))
   AND NOT EXISTS (SELECT 1 FROM authoring.edit_decisions d
                   WHERE d.attempt_id = h.work_id AND d.accounted_at IS NULL)
 ORDER BY h.updated_at
 LIMIT $4
