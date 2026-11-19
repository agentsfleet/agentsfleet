SELECT EXISTS (
    SELECT 1 FROM setup.messages
     WHERE draft_id = $1 AND status IN ($2, $3)
)
