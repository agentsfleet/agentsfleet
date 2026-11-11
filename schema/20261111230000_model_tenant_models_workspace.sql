-- Preserve the credential location used by earlier releases exactly once.
-- Entries without a workspace remain unavailable; runtime never selects a
-- replacement workspace. No credential or model identity is moved or copied.
ALTER TABLE model.tenant_models ADD COLUMN workspace_id UUID;

UPDATE model.tenant_models m
   SET workspace_id = (
       SELECT w.id FROM identity.workspaces w
        WHERE w.tenant_id = m.tenant_id
        ORDER BY w.created_at, w.id LIMIT 1
   );

ALTER TABLE model.tenant_models
    ADD CONSTRAINT fk_tenant_models_workspace
        FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES identity.workspaces (id, tenant_id)
        ON DELETE SET NULL (workspace_id),
    DROP CONSTRAINT uq_tenant_models_tenant_id_model_name_secret_ref,
    ADD CONSTRAINT uq_tenant_models_tenant_workspace_model_secret
        UNIQUE (tenant_id, workspace_id, model_name, secret_ref);

CREATE INDEX idx_tenant_models_workspace_id
    ON model.tenant_models (workspace_id) WHERE workspace_id IS NOT NULL;
