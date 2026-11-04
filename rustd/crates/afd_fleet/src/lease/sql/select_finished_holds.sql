SELECT h.work_id::text AS lease_id, l.status
FROM billing.holds h
LEFT JOIN runner.leases l ON l.id = h.work_id
WHERE h.work_type = $1 AND h.released_at IS NULL
  AND (l.id IS NULL OR (l.status <> $2 AND l.updated_at < now() - $3::interval))
  AND NOT EXISTS (
      SELECT 1 FROM runner.decision_calls d
      WHERE d.original_lease_id = h.work_id AND d.accounted_at IS NULL
  )
ORDER BY h.updated_at
LIMIT $4
