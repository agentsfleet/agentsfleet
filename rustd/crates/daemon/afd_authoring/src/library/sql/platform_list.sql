SELECT l.id::text AS id,
       l.platform_slug,
       l.name,
       l.description,
       l.credential_reasons::text AS credential_reasons,
       l.updated_at,
       l.visibility,
       l.head_revision_id::text AS head_revision_id,
       l.resource_version,
       l.created_at
  FROM authoring.library_entries l
 WHERE l.platform_slug IS NOT NULL AND l.visibility IN ($1, $2)
 ORDER BY l.created_at DESC, l.id DESC
