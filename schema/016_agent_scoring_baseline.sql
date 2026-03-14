-- Agent scoring: workspace latency baseline and feature flag (M9_001)

CREATE TABLE workspace_latency_baseline (
    workspace_id    UUID PRIMARY KEY REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    p50_seconds     BIGINT NOT NULL DEFAULT 0,
    p95_seconds     BIGINT NOT NULL DEFAULT 0,
    sample_count    INTEGER NOT NULL DEFAULT 0,
    computed_at     BIGINT NOT NULL
);

ALTER TABLE workspace_entitlements
    ADD COLUMN enable_agent_scoring BOOLEAN NOT NULL DEFAULT FALSE;

GRANT SELECT, INSERT, UPDATE, DELETE ON workspace_latency_baseline TO worker_accessor;
GRANT SELECT ON workspace_latency_baseline TO api_accessor;
