-- Tenant isolation hardening + prompt lifecycle observability (clean-state)

CREATE TABLE IF NOT EXISTS core.prompt_lifecycle_events (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_prompt_lifecycle_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    event_id            TEXT NOT NULL UNIQUE,
    event_type          TEXT NOT NULL,
    workspace_id        UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    tenant_id           UUID NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    agent_id            UUID,
    config_version_id   UUID,
    metadata_json       TEXT NOT NULL DEFAULT '{}',
    created_at          BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_prompt_lifecycle_events_workspace
    ON core.prompt_lifecycle_events(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_prompt_lifecycle_events_tenant
    ON core.prompt_lifecycle_events(tenant_id, created_at DESC);

CREATE OR REPLACE FUNCTION reject_prompt_lifecycle_event_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'core.prompt_lifecycle_events is append-only';
END;
$$;

DO $$ BEGIN
    CREATE TRIGGER trg_prompt_lifecycle_events_no_update
        BEFORE UPDATE ON core.prompt_lifecycle_events
        FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation();
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

DO $$ BEGIN
    CREATE TRIGGER trg_prompt_lifecycle_events_no_delete
        BEFORE DELETE ON core.prompt_lifecycle_events
        FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation();
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- NOTE: EXCEPTION WHEN duplicate_object silently skips if the policy already exists.
-- To change a policy definition, write a new migration that DROPs the old policy first.

GRANT SELECT, INSERT ON core.prompt_lifecycle_events TO api_runtime, worker_runtime;
