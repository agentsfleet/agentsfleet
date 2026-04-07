-- Agent scoring persistence and read API support (M9_002)

CREATE TABLE IF NOT EXISTS agent.agent_run_scores (
    score_id         UUID PRIMARY KEY,
    run_id           UUID NOT NULL REFERENCES core.runs(run_id) ON DELETE CASCADE,
    agent_id         UUID NOT NULL REFERENCES agent.agent_profiles(agent_id) ON DELETE CASCADE,
    workspace_id     UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    proposal_id      UUID REFERENCES agent.agent_improvement_proposals(proposal_id) ON DELETE SET NULL,
    score            INTEGER NOT NULL CHECK (score >= 0 AND score <= 100),
    axis_scores      TEXT NOT NULL,
    weight_snapshot  TEXT NOT NULL,
    scored_at        BIGINT NOT NULL,
    UNIQUE (run_id),
    CONSTRAINT ck_agent_run_scores_uuidv7 CHECK (substring(score_id::text from 15 for 1) = '7')
);

CREATE INDEX IF NOT EXISTS idx_agent_run_scores_agent
    ON agent.agent_run_scores(agent_id, scored_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_run_scores_workspace
    ON agent.agent_run_scores(workspace_id, score DESC, scored_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_run_scores_proposal
    ON agent.agent_run_scores(proposal_id, scored_at DESC)
    WHERE proposal_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE ON agent.agent_run_scores TO worker_runtime;
GRANT SELECT ON agent.agent_run_scores TO api_runtime;
