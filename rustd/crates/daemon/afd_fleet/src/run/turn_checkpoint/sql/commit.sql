UPDATE runner.checkpoints
SET manifest_root = $2, covered_fold_ids = $3, covered_fold_sequences = $4,
    root_nonce = $5, root_sealed = $6, ready_at = clock_timestamp(), updated_at = now()
WHERE id = $1 AND ready_at IS NULL AND abandoned_at IS NULL
