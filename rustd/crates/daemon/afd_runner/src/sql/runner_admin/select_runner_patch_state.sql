SELECT status, capability_report::text AS capability_report,
       decision_platform_model_id AS decision_model_id, tenant_id
FROM runner.runners
WHERE id = $1 AND ($2::uuid IS NULL OR tenant_id = $2)
FOR UPDATE
