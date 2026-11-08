SELECT status, capability_report::text AS capability_report,
       decision_platform_model_id AS decision_model_id
FROM runner.runners
WHERE id = $1
FOR UPDATE
