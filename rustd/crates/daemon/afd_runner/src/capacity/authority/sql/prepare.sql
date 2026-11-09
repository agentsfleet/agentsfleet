SELECT connection_id, token_hash = $2 AS credential_matches, status = $3 AS active,
       ($6 OR connection_id IS NOT DISTINCT FROM $4
           OR (connection_id IS NOT DISTINCT FROM $5 AND $5 IS NOT NULL)) AS can_resume
FROM runner.runners
WHERE id = $1
