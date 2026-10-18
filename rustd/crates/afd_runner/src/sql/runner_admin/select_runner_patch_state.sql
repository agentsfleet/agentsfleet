SELECT status, capability_report::text AS capability_report
FROM runner.runners
WHERE id = $1::uuid
FOR UPDATE
