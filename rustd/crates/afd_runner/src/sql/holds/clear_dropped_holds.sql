UPDATE runner.fleet_slots
SET hold_expires_at = NULL, updated_at = now()
WHERE last_runner_id = $1::uuid
  AND hold_expires_at IS NOT NULL
  AND ($4::boolean OR updated_at <= statement_timestamp() - $3::interval)
  AND NOT (fleet_id = ANY(($2::text[])::uuid[]))
