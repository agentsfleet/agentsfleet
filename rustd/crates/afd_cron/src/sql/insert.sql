INSERT INTO fleet.schedules
       (id, fleet_id, source, provider_schedule_id, cron_expression, timezone, message,
        desired_status, sync_status, resource_version, sync_token, sync_lease_expires_at,
        last_error, created_at, updated_at, once)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11::uuid,
        clock_timestamp() + $12::interval, NULL, now(), now(), $13)
RETURNING
       id::text, fleet_id::text, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token::text, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text
