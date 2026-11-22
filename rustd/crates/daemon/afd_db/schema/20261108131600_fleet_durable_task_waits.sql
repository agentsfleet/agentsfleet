-- Native dependency waits own wakeup eligibility, not a second work queue.
CREATE TABLE fleet.durable_task_waits (
    id UUID PRIMARY KEY,
    durable_task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    owner_run_id UUID NOT NULL,
    durable_task_generation BIGINT NOT NULL CHECK (durable_task_generation > 0),
    stop_generation BIGINT NOT NULL CHECK (stop_generation >= 0),
    generation BIGINT NOT NULL CHECK (generation > 0),
    github_installation_id BIGINT NOT NULL CHECK (github_installation_id > 0),
    github_repository_id BIGINT NOT NULL CHECK (github_repository_id > 0),
    pull_request_number INTEGER NOT NULL CHECK (pull_request_number > 0),
    expected_head TEXT NOT NULL,
    checks JSONB NOT NULL,
    baseline_digest TEXT NOT NULL,
    due_at TIMESTAMPTZ(3) NOT NULL,
    expires_at TIMESTAMPTZ(3) NOT NULL,
    cooldown_expires_at TIMESTAMPTZ(3) NOT NULL,
    fallback_count INTEGER NOT NULL CHECK (fallback_count >= 0),
    initial_read_at TIMESTAMPTZ(3),
    signalled_at TIMESTAMPTZ(3),
    signal_sequence BIGINT NOT NULL CHECK (signal_sequence >= 0),
    observed_signal_sequence BIGINT NOT NULL CHECK (observed_signal_sequence >= 0),
    claim_id UUID,
    claim_expires_at TIMESTAMPTZ(3),
    consumed_at TIMESTAMPTZ(3),
    cancelled_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_durable_task_wait_scope FOREIGN KEY (durable_task_id, fleet_id, workspace_id)
        REFERENCES fleet.durable_tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT uq_durable_task_wait_owner UNIQUE (owner_run_id),
    CONSTRAINT uq_durable_task_wait_generation UNIQUE (durable_task_id, generation),
    CONSTRAINT ck_durable_task_wait_claim CHECK ((claim_id IS NULL) = (claim_expires_at IS NULL))
);

CREATE UNIQUE INDEX uq_durable_task_wait_current ON fleet.durable_task_waits (durable_task_id)
    WHERE consumed_at IS NULL AND cancelled_at IS NULL;

CREATE INDEX idx_durable_task_wait_due ON fleet.durable_task_waits (due_at, id)
    WHERE consumed_at IS NULL AND cancelled_at IS NULL;

CREATE INDEX idx_durable_task_wait_completion ON fleet.durable_task_waits (workspace_id, github_installation_id, github_repository_id, expected_head, id)
    WHERE consumed_at IS NULL AND cancelled_at IS NULL;

CREATE INDEX idx_durable_task_wait_fleet ON fleet.durable_task_waits (fleet_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.durable_task_waits TO api_runtime;
