-- What each fleet event cost, one row per charge type, accumulating per renewal.
-- Read by the usage export, the billing page and the fleet's budget counter.
--
-- No foreign keys: a charge outlives the fleet, workspace and tenant it names
-- (deletion matrix), so `tenant_id`, `workspace_id` and `fleet_id` are opaque
-- values and `fleet_name` is the name at charge time, kept for the export after a
-- purge. `event_created_at` is the producer's instant the charge belongs to.

CREATE TABLE billing.fleet_charges (
    id                        UUID PRIMARY KEY,
    CONSTRAINT ck_fleet_charges_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id                 UUID   NOT NULL,
    workspace_id              UUID   NOT NULL,
    fleet_id                  UUID   NOT NULL,
    fleet_name                TEXT   NOT NULL,
    event_id                  TEXT   NOT NULL,
    charge_type               TEXT   NOT NULL,
    posture                   TEXT   NOT NULL,
    model                     TEXT   NOT NULL,
    charged_nanos             BIGINT NOT NULL,
    input_token_count         BIGINT,
    cached_input_token_count  BIGINT,
    output_token_count        BIGINT,
    wall_ms                   BIGINT,
    event_created_at          TIMESTAMPTZ(3) NOT NULL,
    created_at                TIMESTAMPTZ(3) NOT NULL,
    updated_at                TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_fleet_charges_event_id_charge_type_fleet_id
        UNIQUE (event_id, charge_type, fleet_id)
);

CREATE INDEX idx_fleet_charges_fleet_id_workspace_id_updated_at
    ON billing.fleet_charges (fleet_id, workspace_id, updated_at);
-- The tenant's usage page, newest first.
CREATE INDEX idx_fleet_charges_tenant_id_created_at_id
    ON billing.fleet_charges (tenant_id, created_at DESC, id DESC);
CREATE INDEX idx_fleet_charges_workspace_id ON billing.fleet_charges (workspace_id);

-- Keeps `fleet.activity_counters.budget_used_nanos` equal to the fleet's charges
-- (rule 5 exception). A purged fleet has no counter row, so nothing is written.
CREATE FUNCTION billing.fleet_charges_bump_budget() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO pg_catalog, fleet, billing
AS $$
DECLARE
    delta BIGINT;
BEGIN
    IF TG_OP = 'INSERT' THEN
        delta := NEW.charged_nanos;
    ELSE
        delta := NEW.charged_nanos - OLD.charged_nanos;
    END IF;
    INSERT INTO fleet.activity_counters
        (fleet_id, event_count, budget_used_nanos, created_at, updated_at)
    SELECT f.id, 0, delta, f.created_at, NEW.updated_at
      FROM fleet.fleets f WHERE f.id = NEW.fleet_id
    ON CONFLICT (fleet_id) DO UPDATE
       SET budget_used_nanos = fleet.activity_counters.budget_used_nanos
                             + EXCLUDED.budget_used_nanos,
           updated_at = GREATEST(fleet.activity_counters.updated_at, EXCLUDED.updated_at);
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_fleet_charges_bump_budget
    AFTER INSERT OR UPDATE OF charged_nanos ON billing.fleet_charges
    FOR EACH ROW EXECUTE FUNCTION billing.fleet_charges_bump_budget();

GRANT SELECT, INSERT, UPDATE ON billing.fleet_charges TO api_runtime;
