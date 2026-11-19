SELECT a.id FROM setup.attempts a
WHERE a.id = $1 AND a.run_id = $2
  AND EXISTS (
    SELECT 1 FROM setup.dispatches d WHERE d.id = $3 AND d.attempt_id = a.id
  )
FOR UPDATE OF a
