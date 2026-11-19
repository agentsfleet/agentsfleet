SELECT worker_count, status, cordon_reason, degraded_reason IS NOT NULL AS degraded, lease_version,
       decision_platform_model_id IS NOT NULL AS decision_selected,
       -- Whether this host may take the work's tenant ($4). A tenant row always names
       -- its tenant, so this is never NULL; locked for no work, a tenant host reads false.
       (platform_owned OR tenant_id IS NOT DISTINCT FROM $4::uuid) AS serves,
       NOT platform_owned AS tenant_owned,
       -- A stream call ($5) must prove the opening credential and connection; a trusted
       -- call was proven upstream. A stream call without its proof is refused.
       (NOT $5::boolean OR (token_hash = $2 AND connection_id IS NOT DISTINCT FROM $3)) AS authorized
FROM runner.runners
WHERE id = $1
FOR UPDATE
