INSERT INTO vault.secrets (id, workspace_id, key_name, dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, secret_tag, kek_version, meta_kind, meta_provider, meta_base_url, meta_has_key, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, now(), now())
ON CONFLICT (workspace_id, key_name) DO NOTHING
