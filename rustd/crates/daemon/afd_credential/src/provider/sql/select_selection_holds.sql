SELECT selected
  FROM model.tenant_models
 WHERE tenant_id = $1 AND id = $2
