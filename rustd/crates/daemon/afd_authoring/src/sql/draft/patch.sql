UPDATE authoring.drafts
   SET title = $2,
       tenant_model_id = $3::uuid,
       instance_name = COALESCE($4, instance_name),
       resource_version = resource_version + 1,
       updated_at = now()
 WHERE id = $1::uuid
