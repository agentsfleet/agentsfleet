SELECT worker_count, status, degraded_reason IS NOT NULL AS degraded, lease_version,
       decision_platform_model_id IS NOT NULL AS decision_selected
FROM runner.runners
WHERE id = $1
FOR UPDATE
