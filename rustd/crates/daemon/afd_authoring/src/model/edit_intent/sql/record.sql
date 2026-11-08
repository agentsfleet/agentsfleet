UPDATE authoring.edit_decisions
SET disposition = $2, usage = $3, charged_nanos = $4,
    accounted_at = CASE WHEN $4::bigint IS NOT NULL THEN clock_timestamp() ELSE NULL END,
    updated_at = clock_timestamp()
WHERE dispatch_id = $1 AND accounted_at IS NULL
