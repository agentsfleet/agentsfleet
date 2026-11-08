SELECT worker_count, status, degraded_reason IS NOT NULL AS degraded, lease_version,
       decision_platform_model_id IS NOT NULL AS decision_selected,
       ((token_hash = $2 AND connection_id IS NOT DISTINCT FROM $3) OR $2 IS NULL) AS authorized
FROM runner.runners
WHERE id = $1
FOR UPDATE
