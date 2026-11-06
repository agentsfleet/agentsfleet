-- Acknowledged evidence fanout stays separate from immutable signed receipts.
-- This cursor never owns execution; only a current wait can append an admission.
CREATE TABLE fleet.task_completion_fanout (
    task_completion_id UUID PRIMARY KEY REFERENCES fleet.task_completions (id) ON DELETE CASCADE,
    after_wait_id UUID,
    next_attempt_at TIMESTAMPTZ(3) NOT NULL,
    finished_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);
CREATE INDEX idx_task_completion_fanout_due ON fleet.task_completion_fanout (next_attempt_at, task_completion_id)
    WHERE finished_at IS NULL;
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_completion_fanout TO api_runtime;
