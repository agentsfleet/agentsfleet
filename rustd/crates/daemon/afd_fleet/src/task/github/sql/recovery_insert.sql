INSERT INTO fleet.task_effect_recovery (task_effect_id, workspace_id, dek_ciphertext, dek_nonce,
    dek_tag, secret_nonce, secret_ciphertext, secret_tag, kek_version, credential_expires_at, due_at, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
    LEAST($10, clock_timestamp() + make_interval(secs => $11)), clock_timestamp(), now(), now())
