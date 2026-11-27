SELECT root_nonce, root_sealed
FROM runner.checkpoints
WHERE id = $1 AND ready_at IS NOT NULL AND abandoned_at IS NULL
