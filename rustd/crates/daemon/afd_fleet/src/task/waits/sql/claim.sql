WITH instant AS MATERIALIZED (SELECT clock_timestamp() AS at)
UPDATE fleet.task_waits SET claim_id = $2, claim_expires_at = instant.at + $3::interval,
    initial_read_at = COALESCE(initial_read_at, instant.at), due_at = instant.at + $3::interval,
    fallback_count = fallback_count + $4::integer, updated_at = now()
FROM instant WHERE id = $1
RETURNING claim_expires_at
