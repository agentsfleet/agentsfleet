UPDATE authoring.messages SET save_error_code = $3, updated_at = now()
 WHERE draft_id = $1::uuid AND sequence = $2
