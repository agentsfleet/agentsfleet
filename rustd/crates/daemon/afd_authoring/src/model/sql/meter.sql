UPDATE authoring.attempts
   SET output_token_count = output_token_count + $2,
       status = CASE WHEN $3 THEN $4 ELSE status END,
       usage_unknown = EXISTS (SELECT 1 FROM authoring.dispatches
                                WHERE attempt_id = $1::uuid AND status IN ($5, $6)),
       updated_at = now()
 WHERE id = $1::uuid
