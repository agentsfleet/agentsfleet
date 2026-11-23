SELECT 1 FROM model.tenant_models
 WHERE tenant_id = $1 AND selected
   FOR UPDATE
