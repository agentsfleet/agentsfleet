UPDATE model.tenant_models
   SET selected = false, updated_at = now()
 WHERE tenant_id = $1 AND selected
