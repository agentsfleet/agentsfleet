UPDATE fleet.fleets
   SET status = $3, name = $4, resource_version = resource_version + 1, updated_at = now()
 WHERE id = $1::uuid AND workspace_id = $2::uuid
