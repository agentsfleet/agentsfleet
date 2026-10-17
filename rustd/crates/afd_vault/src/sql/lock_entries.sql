SELECT id::text FROM model.tenant_models
 WHERE tenant_id = $1::uuid AND secret_ref = $2
 ORDER BY id
   FOR UPDATE
