DELETE FROM model.tenant_models
 WHERE id = $1::uuid AND tenant_id = $2::uuid
