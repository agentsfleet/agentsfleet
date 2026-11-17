SELECT status, capability_report::text AS capability_report,
       decision_platform_model_id AS decision_model_id, platform_owned
FROM runner.runners
WHERE id = $1 AND ($3::boolean OR tenant_id = $2::uuid)
FOR UPDATE
