-- Mutable acknowledgements, kept apart so they cannot alter the immutable publication request.
CREATE TABLE fleet.task_effect_states (
    task_effect_id UUID PRIMARY KEY REFERENCES fleet.task_effects (id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    commit_oid TEXT,
    issued_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE ON fleet.task_effect_states TO api_runtime;
