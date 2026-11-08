UPDATE authoring.drafts
   SET head_candidate_id = $2::uuid,
       resource_version = resource_version + 1,
       updated_at = now()
 WHERE id = $1::uuid
