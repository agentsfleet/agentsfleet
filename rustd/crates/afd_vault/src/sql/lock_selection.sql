SELECT 1 FROM model.tenant_selections
 WHERE tenant_id = $1::uuid
   FOR UPDATE
