UPDATE authoring.drafts
   SET plugin_revision_id = $2, resource_version = resource_version + 1, updated_at = now()
 WHERE id = $1
