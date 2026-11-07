SELECT id::text AS id, model_name, secret_ref, created_at
  FROM model.tenant_models
 WHERE tenant_id = $1::uuid
   AND (created_at, id) < ($2::timestamptz, $3::uuid)
 ORDER BY created_at DESC, id DESC
 LIMIT $4
