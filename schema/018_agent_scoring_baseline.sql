-- Agent scoring: workspace latency baseline (M9_001)

CREATE TABLE agent.workspace_latency_baseline (
    workspace_id    UUID PRIMARY KEY REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    p50_seconds     BIGINT NOT NULL DEFAULT 0,
    p95_seconds     BIGINT NOT NULL DEFAULT 0,
    sample_count    INTEGER NOT NULL DEFAULT 0,
    computed_at     BIGINT NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON agent.workspace_latency_baseline TO worker_runtime;
GRANT SELECT ON agent.workspace_latency_baseline TO api_runtime;
