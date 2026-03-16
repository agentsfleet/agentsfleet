CREATE TABLE agent_run_analysis (
    analysis_id        UUID PRIMARY KEY,
    run_id             UUID NOT NULL UNIQUE REFERENCES runs(run_id) ON DELETE CASCADE,
    agent_id           UUID NOT NULL REFERENCES agent_profiles(agent_id) ON DELETE CASCADE,
    workspace_id       UUID NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
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

CREATE INDEX idx_agent_run_analysis_agent
    ON agent_run_analysis(agent_id, analyzed_at DESC);
CREATE INDEX idx_agent_run_analysis_hints_gin
    ON agent_run_analysis USING GIN (improvement_hints);

ALTER TABLE workspace_entitlements
    ADD COLUMN IF NOT EXISTS enable_score_context_injection BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE workspace_entitlements
    ADD COLUMN IF NOT EXISTS scoring_context_max_tokens INTEGER NOT NULL DEFAULT 2048;
ALTER TABLE workspace_entitlements
    ADD CONSTRAINT ck_workspace_entitlements_scoring_context_max_tokens
    CHECK (scoring_context_max_tokens >= 512 AND scoring_context_max_tokens <= 8192);

GRANT SELECT, INSERT ON agent_run_analysis TO worker_accessor;
GRANT SELECT ON agent_run_analysis TO api_accessor;
