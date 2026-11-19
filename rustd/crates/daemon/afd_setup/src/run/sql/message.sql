SELECT EXISTS (
    SELECT 1 FROM setup.messages
     WHERE draft_id = $1 AND sequence = $2 AND status = $3
)
