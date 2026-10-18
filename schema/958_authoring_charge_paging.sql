-- Settlement identity carries the tenant so paging can seek before joining
-- private draft links and public fleet details. Monetary facts stay intact.
ALTER TABLE billing.authoring_settlements ADD COLUMN tenant_id UUID;

UPDATE billing.authoring_settlements s
SET tenant_id = a.tenant_id
FROM fleet.authoring_attempts a
WHERE a.id = s.attempt_id;

ALTER TABLE billing.authoring_settlements ALTER COLUMN tenant_id SET NOT NULL;

-- Derive the indexed identity from the authoritative attempt on every insert,
-- including reports from runners and daemons already serving during rollout.
CREATE FUNCTION billing.scope_authoring_settlement() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    SELECT a.tenant_id INTO STRICT NEW.tenant_id
    FROM fleet.authoring_attempts a
    WHERE a.id = NEW.attempt_id;
    RETURN NEW;
END;
$$;

REVOKE ALL ON FUNCTION billing.scope_authoring_settlement() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION billing.scope_authoring_settlement() TO api_runtime;

CREATE TRIGGER trg_authoring_settlements_scope
BEFORE INSERT ON billing.authoring_settlements
FOR EACH ROW EXECUTE FUNCTION billing.scope_authoring_settlement();

CREATE INDEX idx_authoring_settlements_tenant_id_created_at_id
ON billing.authoring_settlements(tenant_id, created_at DESC, id DESC);
