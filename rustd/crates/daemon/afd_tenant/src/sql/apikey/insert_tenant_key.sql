INSERT INTO identity.api_keys (id, tenant_id, name, description, key_hash, created_by_subject, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, now(), now())
RETURNING created_at
