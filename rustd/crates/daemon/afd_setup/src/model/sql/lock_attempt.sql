SELECT a.id FROM authoring.attempts a
WHERE a.id = $1 AND a.lease_id = $2
  AND EXISTS (
    SELECT 1 FROM authoring.dispatches d WHERE d.id = $3 AND d.attempt_id = a.id
  )
FOR UPDATE OF a
