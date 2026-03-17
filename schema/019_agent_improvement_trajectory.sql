-- Agent improvement trajectory measurement support (M9_004 section 6)

ALTER TABLE agent_run_scores
    ADD COLUMN IF NOT EXISTS proposal_id UUID REFERENCES agent_improvement_proposals(proposal_id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_agent_run_scores_proposal
    ON agent_run_scores(proposal_id, scored_at DESC)
    WHERE proposal_id IS NOT NULL;
