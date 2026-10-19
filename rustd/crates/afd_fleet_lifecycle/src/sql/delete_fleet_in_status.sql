DELETE FROM fleet.fleets
WHERE id = $1::uuid AND workspace_id = $2::uuid AND status = $3
RETURNING id::text
