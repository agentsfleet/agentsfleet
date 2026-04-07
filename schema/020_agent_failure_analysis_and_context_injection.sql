CREATE TABLE IF NOT EXISTS agent.agent_run_analysis (
    analysis_id        UUID PRIMARY KEY,
    run_id             UUID NOT NULL UNIQUE REFERENCES core.runs(run_id) ON DELETE CASCADE,
    agent_id           UUID NOT NULL REFERENCES agent.agent_profiles(agent_id) ON DELETE CASCADE,
    workspace_id       UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    failure_class      TEXT,
    failure_is_infra   BOOLEAN NOT NULL DEFAULT FALSE,
    failure_signals    JSONB NOT NULL DEFAULT '[]'::jsonb,
    improvement_hints  JSONB NOT NULL DEFAULT '[]'::jsonb,
    stderr_tail        TEXT,
    analyzed_at        BIGINT NOT NULL,
    CONSTRAINT ck_agent_run_analysis_uuidv7 CHECK (substring(analysis_id::text from 15 for 1) = '7'),
    CONSTRAINT ck_failure_signals_array CHECK (jsonb_typeof(failure_signals) = 'array'),
    CONSTRAINT ck_improvement_hints_array CHECK (jsonb_typeof(improvement_hints) = 'array')
);

CREATE INDEX IF NOT EXISTS idx_agent_run_analysis_agent
    ON agent.agent_run_analysis(agent_id, analyzed_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_run_analysis_hints_gin
    ON agent.agent_run_analysis USING GIN (improvement_hints);

GRANT SELECT, INSERT ON agent.agent_run_analysis TO worker_runtime;
GRANT SELECT ON agent.agent_run_analysis TO api_runtime;

CREATE TABLE IF NOT EXISTS IF NOT EXISTS audit.ops_ro_access_events (
    event_id          UUID PRIMARY KEY,
    principal_role    TEXT NOT NULL,
    principal_session TEXT NOT NULL,
    view_name         TEXT NOT NULL,
    app_name          TEXT,
    client_addr       TEXT,
    accessed_at       BIGINT NOT NULL
);

CREATE OR REPLACE FUNCTION audit.log_ops_ro_access(p_view_name TEXT)
RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = audit, pg_catalog
AS $$
DECLARE
    access_event_id UUID := md5(random()::TEXT || clock_timestamp()::TEXT)::UUID;
BEGIN
    INSERT INTO audit.ops_ro_access_events (
        event_id,
        principal_role,
        principal_session,
        view_name,
        app_name,
        client_addr,
        accessed_at
    )
    VALUES (
        access_event_id,
        session_user,
        session_user,
        p_view_name,
        current_setting('application_name', true),
        inet_client_addr()::TEXT,
        (extract(epoch FROM now()) * 1000)::BIGINT
    );
    RETURN TRUE;
END;
$$;

REVOKE ALL ON FUNCTION audit.log_ops_ro_access(TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION audit.log_ops_ro_access(TEXT) TO ops_readonly_human, ops_readonly_agent;

CREATE OR REPLACE VIEW ops_ro.workspace_overview AS
WITH access_mark AS (
    SELECT audit.log_ops_ro_access('ops_ro.workspace_overview')
)
SELECT
    w.workspace_id,
    w.tenant_id,
    -- Redact full repository path for readonly surfaces.
    regexp_replace(w.repo_url, '^(https?://[^/]+/[^/]+/).*$', '\\1***') AS repo_url_masked,
    w.default_branch,
    w.paused,
    w.created_at,
    w.updated_at
FROM core.workspaces AS w
CROSS JOIN access_mark;

CREATE OR REPLACE VIEW ops_ro.run_overview AS
WITH access_mark AS (
    SELECT audit.log_ops_ro_access('ops_ro.run_overview')
)
SELECT
    r.run_id,
    r.workspace_id,
    r.tenant_id,
    r.state,
    r.attempt,
    r.mode,
    r.requested_by,
    r.request_id,
    r.trace_id,
    r.branch,
    r.created_at,
    r.updated_at
FROM core.runs AS r
CROSS JOIN access_mark;

CREATE OR REPLACE VIEW ops_ro.billing_overview AS
WITH access_mark AS (
    SELECT audit.log_ops_ro_access('ops_ro.billing_overview')
)
SELECT
    b.workspace_id,
    b.plan_tier,
    b.billing_status,
    b.adapter,
    b.grace_expires_at,
    b.updated_at
FROM billing.workspace_billing_state AS b
CROSS JOIN access_mark;

GRANT SELECT ON
    ops_ro.workspace_overview,
    ops_ro.run_overview,
    ops_ro.billing_overview
TO ops_readonly_human, ops_readonly_agent;

REVOKE ALL ON vault.secrets FROM ops_readonly_human, ops_readonly_agent;
REVOKE ALL ON vault.workspace_skill_secrets FROM ops_readonly_human, ops_readonly_agent;
