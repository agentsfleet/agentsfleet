-- An enrolled host. Runners hold no datastore credential; agentsfleetd writes this row for them.
-- Degraded is `degraded_reason IS NOT NULL`. `lease_version` serializes the choice of the next
-- lease type, so a stale poll cannot overwrite the alternation another daemon set.
CREATE TABLE runner.runners (
    id                       UUID PRIMARY KEY,
    CONSTRAINT ck_runners_id_uuidv7 CHECK (uuid_extract_version(id) IS NOT DISTINCT FROM 7),
    host_id                  TEXT    NOT NULL,
    token_hash               TEXT    NOT NULL,
    -- One native runner session owns grants and renewals until replaced.
    connection_id            UUID,
    CONSTRAINT ck_runners_connection_uuidv7 CHECK (
        connection_id IS NULL OR uuid_extract_version(connection_id) IS NOT DISTINCT FROM 7
    ),
    sandbox_tier             TEXT    NOT NULL,
    status                   TEXT    NOT NULL,
    labels                   JSONB   NOT NULL,
    network_policy           TEXT    NOT NULL,
    registry_allowlist       JSONB   NOT NULL,
    worker_count             INTEGER NOT NULL,
    sandbox_config           JSONB,
    capability_report        JSONB,
    degraded_reason          TEXT,
    last_seen_at             TIMESTAMPTZ(3)  NOT NULL,
    selftest_requested_at    TIMESTAMPTZ(3),
    selftest_completed_at    TIMESTAMPTZ(3),
    -- NULL means no self-test yet, which is distinct from a failed one.
    selftest_checks          JSONB,
    selftest_sandbox_tier    TEXT,
    selftest_network_policy  TEXT,
    lease_version            BIGINT  NOT NULL,
    last_lease_type          TEXT,
    created_at               TIMESTAMPTZ(3)  NOT NULL,
    updated_at               TIMESTAMPTZ(3)  NOT NULL,
    CONSTRAINT uq_runners_token_hash UNIQUE (token_hash),
    -- The operator assigns a catalogue identity here, never provider credentials.
    decision_platform_model_id UUID
        REFERENCES model.platform_models (id) ON DELETE RESTRICT,
    -- Gates new lease eligibility only, never the authority of an issued lease.
    durable_tasks_enabled BOOLEAN NOT NULL DEFAULT true,
    -- Ownership is stated at registration, never inferred from a missing tenant: a
    -- platform runner takes every tenant's work, so it must be named as one. Both
    -- columns are fixed after insert (trigger below).
    platform_owned BOOLEAN NOT NULL,
    tenant_id UUID REFERENCES identity.tenants (id) ON DELETE CASCADE,
    CONSTRAINT ck_runners_owner CHECK (platform_owned = (tenant_id IS NULL)),
    -- A platform decision model would hand the platform decision key to a
    -- host its tenant holds root on.
    CONSTRAINT ck_runners_tenant_no_decision_model
        CHECK (platform_owned OR decision_platform_model_id IS NULL)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.runners TO api_runtime;

CREATE INDEX idx_runners_decision_platform_model
    ON runner.runners (decision_platform_model_id)
    WHERE decision_platform_model_id IS NOT NULL;

-- The runner list pages newest first; a tenant's list also serves erasure's cascade.
CREATE INDEX idx_runners_created_at_id ON runner.runners (created_at DESC, id DESC);

CREATE INDEX idx_runners_tenant_id_created_at_id
    ON runner.runners (tenant_id, created_at DESC, id DESC)
    WHERE tenant_id IS NOT NULL;

CREATE FUNCTION runner.runners_refuse_owner_change() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'runner ownership is fixed at registration';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_runners_refuse_owner_change
    BEFORE UPDATE OF platform_owned, tenant_id ON runner.runners
    FOR EACH ROW WHEN (OLD.platform_owned IS DISTINCT FROM NEW.platform_owned
                       OR OLD.tenant_id IS DISTINCT FROM NEW.tenant_id)
    EXECUTE FUNCTION runner.runners_refuse_owner_change();
