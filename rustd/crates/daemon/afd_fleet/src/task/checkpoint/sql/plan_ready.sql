UPDATE fleet.task_checkpoints SET manifest_root = $2, history_digest = $3,
    covered_input_sequence = $4, effect_watermark = $5, ready_at = clock_timestamp(), updated_at = now()
WHERE id = $1 AND ready_at IS NULL AND abandoned_at IS NULL
