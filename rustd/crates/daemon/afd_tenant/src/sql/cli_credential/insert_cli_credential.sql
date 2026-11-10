INSERT INTO identity.cli_credentials (id, user_id, tenant_id, machine_name, credential_hash, credential_prefix, deployment, created_from_address, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now())
