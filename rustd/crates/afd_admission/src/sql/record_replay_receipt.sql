UPDATE fleet.admissions
SET receipt = $2, replay_count = replay_count + 1, updated_at = now()
WHERE id = $1::uuid AND receipt IS NULL
RETURNING replay_count
