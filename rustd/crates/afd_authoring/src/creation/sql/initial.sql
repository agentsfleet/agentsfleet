UPDATE fleet.fleets
   SET initial_start = $3::jsonb, updated_at = now()
 WHERE id = $1::uuid AND workspace_id = $2::uuid
