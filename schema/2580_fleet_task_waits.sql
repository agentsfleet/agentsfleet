-- Native dependency waits own wakeup eligibility, not a second work queue.
CREATE TABLE fleet.task_waits (
    id UUID PRIMARY KEY,
    CONSTRAINT ck_task_wait_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    task_id UUID NOT NULL,
    fleet_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    owner_lease_id UUID NOT NULL,
    task_generation BIGINT NOT NULL CHECK (task_generation > 0),
    stop_generation BIGINT NOT NULL CHECK (stop_generation >= 0),
    generation BIGINT NOT NULL CHECK (generation > 0),
    installation_id BIGINT NOT NULL CHECK (installation_id > 0),
    repository_id BIGINT NOT NULL CHECK (repository_id > 0),
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
    observed_signal_at TIMESTAMPTZ(3),
    signal_sequence BIGINT NOT NULL CHECK (signal_sequence >= 0),
    observed_signal_sequence BIGINT NOT NULL CHECK (observed_signal_sequence >= 0),
    claim_id UUID,
    claim_expires_at TIMESTAMPTZ(3),
    consumed_admission_id UUID,
    consumed_at TIMESTAMPTZ(3),
    cancelled_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_task_wait_scope FOREIGN KEY (task_id, fleet_id, workspace_id)
        REFERENCES fleet.tasks (id, fleet_id, workspace_id) ON DELETE CASCADE,
    CONSTRAINT uq_task_wait_owner UNIQUE (owner_lease_id),
    CONSTRAINT uq_task_wait_generation UNIQUE (task_id, generation),
    CONSTRAINT ck_task_wait_claim CHECK ((claim_id IS NULL) = (claim_expires_at IS NULL)),
    CONSTRAINT ck_task_wait_consumed CHECK ((consumed_at IS NULL) = (consumed_admission_id IS NULL))
);
CREATE UNIQUE INDEX uq_task_wait_current ON fleet.task_waits (task_id)
    WHERE consumed_at IS NULL AND cancelled_at IS NULL;
CREATE INDEX idx_task_wait_due ON fleet.task_waits (due_at, id)
    WHERE consumed_at IS NULL AND cancelled_at IS NULL;
CREATE INDEX idx_task_wait_completion ON fleet.task_waits (workspace_id, installation_id, repository_id, expected_head, id)
    WHERE consumed_at IS NULL AND cancelled_at IS NULL;
CREATE INDEX idx_task_wait_fleet ON fleet.task_waits (fleet_id);
GRANT SELECT, INSERT, UPDATE, DELETE ON fleet.task_waits TO api_runtime;
