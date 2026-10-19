INSERT INTO authoring.library_entries
    (id, tenant_id, visibility, name, description, head_revision_id, resource_version,
     created_at, updated_at, platform_slug)
VALUES ($1::uuid, NULL, $2, $3, $4, $5::uuid, $6, now(), now(), $7)
ON CONFLICT (platform_slug) WHERE platform_slug IS NOT NULL DO NOTHING
RETURNING id::text
