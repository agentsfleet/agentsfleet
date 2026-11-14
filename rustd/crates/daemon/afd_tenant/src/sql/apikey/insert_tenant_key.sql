INSERT INTO identity.api_keys (id, tenant_id, name, description, key_hash, display_suffix, created_by_subject, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
RETURNING created_at
