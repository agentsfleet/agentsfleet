UPDATE authoring.drafts
   SET status = $2, closed_at = now(), expires_at = now() + $3::interval,
       resource_version = resource_version + 1, updated_at = now()
 WHERE id = $1::uuid
