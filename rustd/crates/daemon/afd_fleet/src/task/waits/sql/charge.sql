UPDATE fleet.tasks SET used_fallback_reads = used_fallback_reads + 1,
    status_reason = CASE WHEN used_fallback_reads + 1 >= max_fallback_reads THEN $2 ELSE status_reason END,
    resource_version = resource_version + 1, updated_at = now()
WHERE id = $1::uuid
