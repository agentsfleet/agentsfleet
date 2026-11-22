-- Mutable acknowledgements, kept apart so they cannot alter the immutable publication request.
CREATE TABLE fleet.durable_task_effect_states (
    durable_task_effect_id UUID PRIMARY KEY REFERENCES fleet.durable_task_effects (id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    commit_oid TEXT,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE ON fleet.durable_task_effect_states TO api_runtime;
