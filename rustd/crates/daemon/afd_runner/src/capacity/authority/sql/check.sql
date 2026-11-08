SELECT token_hash = $2 AS credential_matches, status = $4 AS active,
       connection_id IS NOT DISTINCT FROM $3 AS owned
FROM runner.runners
WHERE id = $1
