SELECT l.id AS id,
       l.name,
       l.description,
       l.credential_reasons::text AS credential_reasons,
       l.head_revision_id AS head_revision_id,
       l.resource_version,
       l.created_at
  FROM authoring.plugin_entries l
 WHERE l.id = $1
   AND l.tenant_id = $2
   FOR UPDATE
