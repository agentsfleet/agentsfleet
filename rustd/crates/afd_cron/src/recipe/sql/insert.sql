INSERT INTO fleet.schedules
       (id, fleet_id, source, provider_schedule_id, cron_expression, timezone, message,
        once, desired_status, sync_status, resource_version, recipe, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, false, $8, $9, 1, $10::jsonb, now(), now())
