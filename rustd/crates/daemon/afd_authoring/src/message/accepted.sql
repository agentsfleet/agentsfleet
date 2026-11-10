SELECT save_requested, user_text FROM authoring.messages
 WHERE draft_id = $1 AND sequence = $2
