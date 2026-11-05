SELECT EXISTS (
    SELECT 1 FROM authoring.edit_decisions
    WHERE attempt_id = $1::uuid AND accounted_at IS NULL
)
