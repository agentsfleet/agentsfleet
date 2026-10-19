UPDATE fleet.fleets
   SET head_revision_id = $2::uuid, status = $3,
       resource_version = resource_version + 1, updated_at = now()
 WHERE id = $1::uuid
