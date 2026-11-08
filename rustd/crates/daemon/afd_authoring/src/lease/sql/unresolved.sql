SELECT EXISTS (
    SELECT 1 FROM authoring.dispatches
     WHERE attempt_id = $1::uuid AND status IN ($2, $3)
)
