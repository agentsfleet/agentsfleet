-- A runner's tenant; NULL is a platform runner. A tenant runner takes only its
-- tenant's work, and the platform decision key must never reach it.
ALTER TABLE runner.runners
    ADD COLUMN tenant_id UUID REFERENCES identity.tenants (id) ON DELETE CASCADE,
    ADD CONSTRAINT ck_runners_tenant_no_decision_model
        CHECK (tenant_id IS NULL OR decision_platform_model_id IS NULL);

CREATE INDEX idx_runners_tenant_id_id
    ON runner.runners (tenant_id, id)
    WHERE tenant_id IS NOT NULL;

-- Fixed at insert: a tenant runner flipped to NULL would receive every tenant's work.
CREATE FUNCTION runner.runners_refuse_tenant_change() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'runners.tenant_id is fixed at registration';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_runners_refuse_tenant_change
    BEFORE UPDATE OF tenant_id ON runner.runners
    FOR EACH ROW WHEN (OLD.tenant_id IS DISTINCT FROM NEW.tenant_id)
    EXECUTE FUNCTION runner.runners_refuse_tenant_change();
