INSERT INTO identity.api_keys
    (id, tenant_id, name, description, key_hash, created_by_subject, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, '', $4, $5,
        coalesce($6::timestamptz, now()), coalesce($6::timestamptz, now()))
