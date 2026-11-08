UPDATE authoring.drafts
   SET status = $2, fleet_id = NULL, closed_at = NULL, expires_at = NULL,
       resource_version = resource_version + 1, updated_at = now()
 WHERE fleet_id = $1::uuid AND status = $3
