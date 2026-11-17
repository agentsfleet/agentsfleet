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
    -- The tenant that enrolled this host; NULL is a platform runner. Fixed after insert
    -- (trigger below), because a tenant runner takes only its tenant's work.
    tenant_id UUID REFERENCES identity.tenants (id) ON DELETE CASCADE,
    -- A platform decision model would hand the platform decision key to a
    -- host its tenant holds root on.
    CONSTRAINT ck_runners_tenant_no_decision_model
        CHECK (tenant_id IS NULL OR decision_platform_model_id IS NULL)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON runner.runners TO api_runtime;

CREATE INDEX idx_runners_decision_platform_model
    ON runner.runners (decision_platform_model_id)
    WHERE decision_platform_model_id IS NOT NULL;

CREATE INDEX idx_runners_tenant_id_id
    ON runner.runners (tenant_id, id)
    WHERE tenant_id IS NOT NULL;

CREATE FUNCTION runner.runners_refuse_tenant_change() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'runners.tenant_id is fixed at registration';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_runners_refuse_tenant_change
    BEFORE UPDATE OF tenant_id ON runner.runners
    FOR EACH ROW WHEN (OLD.tenant_id IS DISTINCT FROM NEW.tenant_id)
    EXECUTE FUNCTION runner.runners_refuse_tenant_change();
