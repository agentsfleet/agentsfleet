SELECT COALESCE(tenant_model_id = $2, false)
  FROM model.tenant_selections
 WHERE tenant_id = $1
