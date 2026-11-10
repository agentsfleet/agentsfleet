INSERT INTO fleet.schedules
       (id, fleet_id, source, provider_schedule_id, cron_expression, timezone, message,
        once, desired_status, sync_status, resource_version, recipe, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $9, 1, $10::jsonb, now(), now())
RETURNING
       id, fleet_id, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text
