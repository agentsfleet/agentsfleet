SELECT EXISTS (
    SELECT 1 FROM authoring.dispatches
     WHERE attempt_id = $1 AND status IN ($2, $3)
)
