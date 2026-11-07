SELECT sequence, tenant_model_id::text AS tenant_model_id
  FROM authoring.messages
 WHERE draft_id = $1::uuid AND status = $2 AND sequence > $3
 ORDER BY sequence
 LIMIT 1
