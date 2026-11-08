SELECT EXISTS (
    SELECT 1 FROM authoring.messages
     WHERE draft_id = $1::uuid AND sequence = $2 AND status = $3
)
