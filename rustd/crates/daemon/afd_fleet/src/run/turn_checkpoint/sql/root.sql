SELECT root_dek_ciphertext AS dek_ciphertext, root_dek_nonce AS dek_nonce,
       root_dek_tag AS dek_tag, root_secret_nonce AS secret_nonce,
       root_secret_ciphertext AS secret_ciphertext, root_secret_tag AS secret_tag,
       root_kek_version AS kek_version
FROM runner.turn_checkpoints
WHERE id = $1 AND ready_at IS NOT NULL AND abandoned_at IS NULL
