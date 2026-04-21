-- M10_001: agent_run_analysis table removed (pipeline v1 scoring).
-- Audit logging infrastructure and ops_ro views retained below.

CREATE TABLE IF NOT EXISTS audit.ops_ro_access_events (
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
    regexp_replace(w.repo_url, '^(https?://[^/]+/[^/]+/).*$', '\\1***') AS repo_url_masked,
    w.default_branch,
    w.paused,
    w.created_at,
    w.updated_at
FROM core.workspaces AS w
CROSS JOIN access_mark;

-- ops_ro.billing_overview removed along with workspace_billing_state.
-- Tenant-scoped billing view can be reintroduced off billing.tenant_billing
-- in a later migration when ops actually needs it.

GRANT SELECT ON
    ops_ro.workspace_overview
TO ops_readonly_human, ops_readonly_agent;

REVOKE ALL ON vault.secrets FROM ops_readonly_human, ops_readonly_agent;
