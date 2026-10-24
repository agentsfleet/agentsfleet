SELECT user_text FROM authoring.messages WHERE draft_id = $1::uuid AND sequence = $2
