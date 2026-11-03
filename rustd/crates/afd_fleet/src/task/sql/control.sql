UPDATE fleet.tasks
SET status = $4, resource_version = resource_version + 1,
    generation = generation + 1,
    stop_generation = stop_generation + $5,
    terminal_at = CASE WHEN $6 THEN now() ELSE terminal_at END,
    updated_at = now()
WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid
  AND resource_version = $7
RETURNING id::text
