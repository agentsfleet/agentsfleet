UPDATE runner.turn_checkpoints
SET manifest_root = $2, covered_fold_ids = $3, covered_fold_sequences = $4,
    root_dek_ciphertext = $5, root_dek_nonce = $6, root_dek_tag = $7,
    root_secret_nonce = $8, root_secret_ciphertext = $9, root_secret_tag = $10,
    root_kek_version = $11, ready_at = clock_timestamp(), updated_at = now()
WHERE id = $1 AND ready_at IS NULL AND abandoned_at IS NULL
