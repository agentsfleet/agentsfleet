-- Mutable acknowledgements cannot alter the immutable publication request.
CREATE TABLE fleet.task_effect_states (
    task_effect_id UUID PRIMARY KEY REFERENCES fleet.task_effects (id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    commit_oid TEXT,
    issued_at TIMESTAMPTZ(3),
    confirmed_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_effect_confirmed_commit CHECK ((confirmed_at IS NULL) = (commit_oid IS NULL))
);
CREATE INDEX idx_task_effect_states_reconcile ON fleet.task_effect_states (updated_at, task_effect_id)
    WHERE issued_at IS NOT NULL AND confirmed_at IS NULL;
GRANT SELECT, INSERT, UPDATE ON fleet.task_effect_states TO api_runtime;
