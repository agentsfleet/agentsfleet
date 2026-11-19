UPDATE authoring.messages SET save_error_code = $3, updated_at = now()
 WHERE draft_id = $1 AND sequence = $2
