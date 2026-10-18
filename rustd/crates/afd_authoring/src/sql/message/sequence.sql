SELECT sequence FROM authoring.messages WHERE id = $1::uuid AND draft_id = $2::uuid
