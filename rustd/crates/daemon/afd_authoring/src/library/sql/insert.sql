INSERT INTO authoring.library_entries
    (id, tenant_id, visibility, name, description, head_revision_id, resource_version,
     created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
