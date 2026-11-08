SELECT EXISTS (
    SELECT 1 FROM authoring.messages
     WHERE draft_id = $1::uuid AND status IN ($2, $3)
)
