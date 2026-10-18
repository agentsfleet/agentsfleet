SELECT EXISTS (
    SELECT 1 FROM authoring.attempts
     WHERE draft_id = $1::uuid AND (status <> $2 OR usage_unknown)
)
