INSERT INTO setup.plugin_entries
    (id, tenant_id, name, description, head_revision_id, resource_version, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, now(), now())
