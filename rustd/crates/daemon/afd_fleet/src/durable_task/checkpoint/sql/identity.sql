SELECT id, key_version, dek_ciphertext, dek_nonce, dek_tag,
       secret_nonce, secret_ciphertext, secret_tag, kek_version
FROM fleet.durable_task_identity_keys
WHERE workspace_id = $1 AND repository = $2 AND retired_at IS NULL
FOR UPDATE
