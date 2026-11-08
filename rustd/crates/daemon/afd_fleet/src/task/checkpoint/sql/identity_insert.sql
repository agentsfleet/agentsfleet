INSERT INTO fleet.task_identity_keys (id, workspace_id, repository, key_version,
    dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, secret_tag, kek_version, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, now(), now())
